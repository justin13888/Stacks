//! The caller's own account: profile, preferences and API tokens.

use std::sync::Arc;

use kynos::extract::link::Link;
use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::auth::{AuthError, AuthService};
use stacks_core::entity::enums::{Role as CoreRole, Scope};
use stacks_core::entity::{api_token, user};
use stacks_core::user::settings as core_settings;
use stacks_core::user::{UserError, UserService, UserSettings};
use uuid::Uuid;

use crate::app::App;
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::Internal;
use crate::security::{Caller, Reader, SessionOnly, Writer};

/// The caller's account.
#[derive(Tag)]
#[tag(
    name = "account",
    description = "The caller's profile, preferences and API tokens."
)]
pub struct Account;

/// What an account may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Admin,
}

impl From<CoreRole> for Role {
    fn from(role: CoreRole) -> Self {
        match role {
            CoreRole::User => Self::User,
            CoreRole::Admin => Self::Admin,
        }
    }
}

impl From<Role> for CoreRole {
    fn from(role: Role) -> Self {
        match role {
            Role::User => Self::User,
            Role::Admin => Self::Admin,
        }
    }
}

/// How scores are shown and entered. Stored scores are always 0–100.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub enum ScoreFormat {
    /// 0–100.
    #[serde(rename = "point_100")]
    Point100,
    /// 0.0–10.0 in steps of 0.5.
    #[serde(rename = "point_10_decimal")]
    Point10Decimal,
    /// 0–10.
    #[serde(rename = "point_10")]
    Point10,
    /// 0–5 stars.
    #[serde(rename = "point_5")]
    Point5,
    /// Disliked, neutral, liked.
    #[serde(rename = "point_3")]
    Point3,
}

/// Which title to show for titles that have several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleLanguage {
    English,
    Romaji,
    Native,
}

/// Whether episode titles and overviews past the caller's progress show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spoilers {
    Hide,
    Show,
}

/// The web app's colour scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// The caller's preferences.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct Preferences {
    pub score_format: ScoreFormat,
    pub title_language: TitleLanguage,
    pub spoilers: Spoilers,
    pub theme: Theme,
}

impl From<UserSettings> for Preferences {
    fn from(s: UserSettings) -> Self {
        Self {
            score_format: match s.score_format {
                core_settings::ScoreFormat::Point100 => ScoreFormat::Point100,
                core_settings::ScoreFormat::Point10Decimal => ScoreFormat::Point10Decimal,
                core_settings::ScoreFormat::Point10 => ScoreFormat::Point10,
                core_settings::ScoreFormat::Point5 => ScoreFormat::Point5,
                core_settings::ScoreFormat::Point3 => ScoreFormat::Point3,
            },
            title_language: match s.title_language {
                core_settings::TitleLanguage::English => TitleLanguage::English,
                core_settings::TitleLanguage::Romaji => TitleLanguage::Romaji,
                core_settings::TitleLanguage::Native => TitleLanguage::Native,
            },
            spoilers: match s.spoilers {
                core_settings::Spoilers::Hide => Spoilers::Hide,
                core_settings::Spoilers::Show => Spoilers::Show,
            },
            theme: match s.theme {
                core_settings::Theme::System => Theme::System,
                core_settings::Theme::Light => Theme::Light,
                core_settings::Theme::Dark => Theme::Dark,
            },
        }
    }
}

/// An account.
#[derive(Clone, Debug, Schema, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub display_name: String,
    /// The email the identity provider reported, if any.
    pub email: Option<String>,
    pub role: Role,
    pub preferences: Preferences,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&user::Model> for User {
    fn from(u: &user::Model) -> Self {
        // A stored document that no longer reads (a value this release does
        // not know) shows as the defaults rather than failing every read of
        // the account; the next PATCH rewrites it.
        let settings = UserSettings::from_json(&u.settings).unwrap_or_else(|e| {
            tracing::warn!(user_id = %u.id, error = %e, "unreadable settings; showing defaults");
            UserSettings::default()
        });
        Self {
            id: u.id,
            display_name: u.display_name.clone(),
            email: u.email.clone(),
            role: u.role.into(),
            preferences: settings.into(),
            created_at: u.created_at.to_utc(),
        }
    }
}

/// A change to the caller's preferences. Members left out are unchanged.
#[derive(Debug, Default, Schema, Serialize, Deserialize)]
pub struct PreferencesPatch {
    pub score_format: Option<ScoreFormat>,
    pub title_language: Option<TitleLanguage>,
    pub spoilers: Option<Spoilers>,
    pub theme: Option<Theme>,
}

impl PreferencesPatch {
    /// The members this patch sets, as the JSON object the stored document
    /// merges.
    fn into_merge(self) -> serde_json::Value {
        let mut merge = serde_json::Map::new();
        let mut set = |key: &str, value: Option<serde_json::Value>| {
            if let Some(value) = value {
                merge.insert(key.to_owned(), value);
            }
        };
        set(
            "score_format",
            self.score_format.and_then(|v| serde_json::to_value(v).ok()),
        );
        set(
            "title_language",
            self.title_language
                .and_then(|v| serde_json::to_value(v).ok()),
        );
        set(
            "spoilers",
            self.spoilers.and_then(|v| serde_json::to_value(v).ok()),
        );
        set(
            "theme",
            self.theme.and_then(|v| serde_json::to_value(v).ok()),
        );
        serde_json::Value::Object(merge)
    }
}

/// A change to the caller's account. Members left out are unchanged.
#[derive(Debug, Default, Schema, Serialize, Deserialize)]
pub struct UserPatch {
    #[schema(min_length = 1, max_length = 100)]
    pub display_name: Option<String>,
    pub preferences: Option<PreferencesPatch>,
}

/// What changing the account can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum UpdateUserError {
    #[error("the display name must not be blank")]
    #[problem(status = 422, title = "Invalid display name")]
    InvalidDisplayName,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// Read the caller's account.
#[kynos::get("/me", operation_id = "getMe")]
async fn get_me(Auth(caller): Reader) -> Json<User> {
    let caller = Caller::from(caller);
    Json(User::from(&caller.user))
}

/// Change the caller's display name or preferences.
///
/// Merge semantics: a member left out is unchanged, and preferences merge
/// member by member.
#[kynos::patch("/me", operation_id = "updateMe")]
async fn update_me(
    Auth(caller): Writer,
    Inject(db): Inject<DatabaseConnection>,
    Json(patch): Json<UserPatch>,
) -> Result<Json<User>, UpdateUserError> {
    let caller = Caller::from(caller);
    let users = UserService::new(db);
    let map = |e: UserError| match e {
        UserError::EmptyDisplayName => UpdateUserError::InvalidDisplayName,
        other => Internal::log(other).into(),
    };
    let mut user = caller.user;
    if let Some(name) = patch.display_name {
        if name.trim().is_empty() {
            return Err(UpdateUserError::InvalidDisplayName);
        }
        user = users
            .update_display_name(user.id, name.trim().to_owned())
            .await
            .map_err(map)?;
    }
    if let Some(preferences) = patch.preferences {
        user = users
            .update_settings(user.id, preferences.into_merge())
            .await
            .map_err(map)?;
    }
    Ok(Json(User::from(&user)))
}

// ---------------------------------------------------------------- API tokens

/// A scope an API token can be granted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    /// Read the owner's data.
    Read,
    /// Change the owner's data.
    Write,
    /// Administer the deployment (administrators only).
    Admin,
}

impl From<TokenScope> for Scope {
    fn from(s: TokenScope) -> Self {
        match s {
            TokenScope::Read => Self::Read,
            TokenScope::Write => Self::Write,
            TokenScope::Admin => Self::Admin,
        }
    }
}

fn token_scope(raw: &str) -> Option<TokenScope> {
    match Scope::parse(raw)? {
        Scope::Read => Some(TokenScope::Read),
        Scope::Write => Some(TokenScope::Write),
        Scope::Admin => Some(TokenScope::Admin),
    }
}

/// An API token, without its secret.
#[derive(Clone, Debug, Schema, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: Uuid,
    pub name: String,
    /// The first characters of the token, to recognise it by.
    pub prefix: String,
    pub scopes: Vec<TokenScope>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// `null` for a token that never expires.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `null` until first used.
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<&api_token::Model> for ApiToken {
    fn from(t: &api_token::Model) -> Self {
        Self {
            id: t.id,
            name: t.name.clone(),
            prefix: t.token_prefix.clone(),
            scopes: t.scopes.iter().filter_map(|s| token_scope(s)).collect(),
            created_at: t.created_at.to_utc(),
            expires_at: t.expires_at.map(|e| e.to_utc()),
            last_used_at: t.last_used_at.map(|e| e.to_utc()),
        }
    }
}

/// A token to create.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct NewApiToken {
    /// What the token is for.
    #[schema(min_length = 1, max_length = 100)]
    pub name: String,
    /// At least one scope.
    #[schema(min_items = 1)]
    pub scopes: Vec<TokenScope>,
    /// When the token stops working; `null` or absent for never.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A token just created, with its secret.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct CreatedApiToken {
    pub token: ApiToken,
    /// The secret, shown this once and never again. Send it as
    /// `Authorization: Bearer <secret>`.
    pub secret: String,
}

/// What `/me/api-tokens/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct TokenPath {
    pub id: Uuid,
}

/// What listing tokens can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListTokensError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// No token of the caller's has that id.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no API token of yours has that id")]
#[problem(base = crate::problem::BASE, status = 404, title = "API token not found")]
pub struct ApiTokenNotFound;

/// What reading or revoking one token can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum TokenError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] ApiTokenNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What creating a token can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum CreateTokenError {
    #[error("{0}")]
    #[problem(status = 422, title = "Invalid API token")]
    InvalidApiToken(String),
    #[error("only an administrator can grant the admin scope")]
    #[problem(status = 422, title = "Scope not permitted")]
    ScopeNotPermitted,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// List the caller's live API tokens, newest first.
#[kynos::get("/me/api-tokens", operation_id = "listApiTokens")]
async fn list_api_tokens(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(auth): Inject<Arc<AuthService>>,
) -> Result<Json<Page<ApiToken>>, ListTokensError> {
    let caller = Caller::from(caller);
    let tokens = auth
        .list_api_tokens(caller.id())
        .await
        .map_err(Internal::log)?;
    let page = page::by_id_desc(tokens, |t| t.id, &params)?;
    Ok(Json(page.map(|t| ApiToken::from(&t))))
}

/// Read one of the caller's API tokens.
#[kynos::get("/me/api-tokens/{id}", operation_id = "getApiToken")]
async fn get_api_token(
    Auth(caller): Reader,
    Path(path): Path<TokenPath>,
    Inject(auth): Inject<Arc<AuthService>>,
) -> Result<Json<ApiToken>, TokenError> {
    let caller = Caller::from(caller);
    let tokens = auth
        .list_api_tokens(caller.id())
        .await
        .map_err(Internal::log)?;
    tokens
        .iter()
        .find(|t| t.id == path.id)
        .map(|t| Json(ApiToken::from(t)))
        .ok_or_else(|| ApiTokenNotFound.into())
}

/// Create an API token.
///
/// The secret is in the response and nowhere else, ever. Only a signed-in
/// browser can create a token, so a leaked token cannot mint another.
#[kynos::post("/me/api-tokens", operation_id = "createApiToken")]
async fn create_api_token(
    Auth(caller): SessionOnly,
    token_link: Link<get_api_token>,
    Inject(auth): Inject<Arc<AuthService>>,
    Json(body): Json<NewApiToken>,
) -> Result<Created<Json<CreatedApiToken>>, CreateTokenError> {
    let scopes: Vec<Scope> = body.scopes.iter().map(|s| Scope::from(*s)).collect();
    let (token, secret) = auth
        .create_api_token(&caller.user, &body.name, &scopes, body.expires_at)
        .await
        .map_err(|e| match e {
            AuthError::InvalidRequest(why) => CreateTokenError::InvalidApiToken(why),
            AuthError::ScopeNotPermitted(_) => CreateTokenError::ScopeNotPermitted,
            other => Internal::log(other).into(),
        })?;
    let location = token_link.uri(TokenPath { id: token.id });
    Ok(Created::at(
        location,
        Json(CreatedApiToken {
            token: ApiToken::from(&token),
            secret,
        }),
    ))
}

/// Revoke an API token. It stops working at once.
#[kynos::delete("/me/api-tokens/{id}", operation_id = "revokeApiToken")]
async fn revoke_api_token(
    Auth(caller): SessionOnly,
    Path(path): Path<TokenPath>,
    Inject(auth): Inject<Arc<AuthService>>,
) -> Result<NoContent, TokenError> {
    let revoked = auth
        .revoke_api_token(caller.user.id, path.id)
        .await
        .map_err(Internal::log)?;
    if revoked {
        Ok(NoContent)
    } else {
        Err(ApiTokenNotFound.into())
    }
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Account>().mount(kynos::routes![
        get_me,
        update_me,
        list_api_tokens,
        get_api_token,
        create_api_token,
        revoke_api_token,
    ])
}
