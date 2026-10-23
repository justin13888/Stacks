//! Signing in and out: the OpenID Connect handshake, and sessions.
//!
//! The web app asks for an authorization URL, sends the browser there, and
//! hands the `code` and `state` the identity provider redirects back with to
//! `createSession`, which verifies them, provisions the account and sets the
//! session cookie. JavaScript never sees a credential.

use std::time::Duration;

use kynos::extract::link::Link;
use kynos::http::forwarded::Forwarded;
use kynos::middleware::cookies::SetCookieHeaders;
use kynos::prelude::*;
use kynos::response::cookie::{Cookie, SameSite};
use serde::{Deserialize, Serialize};
use stacks_core::auth::{AuthError, AuthService, SessionMeta};
use stacks_core::entity::session;
use std::sync::Arc;
use uuid::Uuid;

use crate::account::User;
use crate::app::{App, Settings};
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::Internal;
use crate::security::{Caller, Reader, SESSION_COOKIE, SessionOnly};

/// Signing in and out.
#[derive(Tag)]
#[tag(
    name = "sessions",
    description = "Sign-in through OpenID Connect, and the sessions it creates."
)]
pub struct Sessions;

/// Where to send the browser to sign in.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct OidcAuthorization {
    /// The identity provider's authorization URL, carrying this request's
    /// `state`, nonce and PKCE challenge.
    pub authorization_url: String,
}

/// What the identity provider redirected back with.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct NewSession {
    /// The authorization code.
    #[schema(min_length = 1, max_length = 4096)]
    pub code: String,
    /// The `state` the authorization request carried.
    #[schema(min_length = 1, max_length = 512)]
    pub state: String,
}

/// A signed-in browser.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    /// Whether this is the session the request was made with.
    pub current: bool,
    /// The browser's `User-Agent` when it signed in.
    pub user_agent: Option<String>,
    /// The client address when it signed in.
    pub ip: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl Session {
    fn from_model(model: &session::Model, current: Option<Uuid>) -> Self {
        Self {
            id: model.id,
            current: current == Some(model.id),
            user_agent: model.user_agent.clone(),
            ip: model.ip.clone(),
            created_at: model.created_at.to_utc(),
            last_seen_at: model.last_seen_at.to_utc(),
            expires_at: model.expires_at.to_utc(),
        }
    }
}

/// The session a request was made with, and who it belongs to.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct CurrentSession {
    pub session: Session,
    pub user: User,
}

/// What a sign-in request sent along.
#[derive(Debug, HeaderParams)]
pub struct ClientHeaders {
    #[header(rename = "User-Agent")]
    pub user_agent: Option<String>,
}

/// What `/sessions/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct SessionPath {
    pub id: Uuid,
}

/// Why a sign-in could not start.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum BeginSignInError {
    #[error("the identity provider is unavailable; try again shortly")]
    #[problem(status = 502, title = "Identity provider unavailable")]
    IdentityProviderUnavailable,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// Why a sign-in could not complete.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum SignInError {
    /// The `state` is unknown, expired or used, or not the one this
    /// browser's `stacks_login` cookie carries; the provider refused the
    /// code; or the id_token failed verification. Start a new sign-in.
    #[error("the sign-in could not be completed; start again")]
    #[problem(status = 400, title = "Sign-in failed")]
    SignInFailed,
    #[error("this account has been deactivated")]
    #[problem(status = 403, title = "Account deactivated")]
    AccountDeactivated,
    #[error("the identity provider is unavailable; try again shortly")]
    #[problem(status = 502, title = "Identity provider unavailable")]
    IdentityProviderUnavailable,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<AuthError> for SignInError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::LoginRejected(why) => {
                tracing::info!(reason = %why, "sign-in rejected");
                Self::SignInFailed
            }
            AuthError::IdentityProvider(why) => {
                tracing::warn!(reason = %why, "identity provider unavailable");
                Self::IdentityProviderUnavailable
            }
            AuthError::Deactivated => Self::AccountDeactivated,
            AuthError::InvalidRequest(_) | AuthError::ScopeNotPermitted(_) => {
                Internal::log(e).into()
            }
            AuthError::Db(e) => Internal::from(e).into(),
        }
    }
}

/// The cookie that carries a session.
fn session_cookie(token: String, ttl: Duration, settings: &Settings) -> Cookie {
    let cookie = Cookie::new(SESSION_COOKIE, token)
        .path("/api")
        .http_only()
        .same_site(SameSite::Lax)
        .max_age(ttl);
    if settings.secure_cookies {
        cookie.secure()
    } else {
        cookie
    }
}

/// The cookie that removes the session cookie.
fn removal_cookie(settings: &Settings) -> Cookie {
    let cookie = Cookie::removal(SESSION_COOKIE)
        .path("/api")
        .http_only()
        .same_site(SameSite::Lax);
    if settings.secure_cookies {
        cookie.secure()
    } else {
        cookie
    }
}

/// The cookie binding a sign-in to the browser that began it: it carries the
/// authorization request's `state`, and `createSession` refuses a `state`
/// this browser was not given.
pub const LOGIN_COOKIE: &str = "stacks_login";

/// Where the sign-in cookie is sent: only to `createSession`.
const LOGIN_COOKIE_PATH: &str = "/api/v1/sessions";

/// The sign-in a browser began, as its cookie records it.
#[derive(Debug, Deserialize, CookieParams)]
pub struct LoginCookie {
    /// The `state` of the authorization request this browser began.
    #[serde(rename = "stacks_login")]
    pub state: Option<String>,
}

fn login_cookie(state: String, settings: &Settings) -> Cookie {
    let ttl = u64::try_from(stacks_core::auth::LOGIN_STATE_TTL.num_seconds()).unwrap_or(600);
    let cookie = Cookie::new(LOGIN_COOKIE, state)
        .path(LOGIN_COOKIE_PATH)
        .http_only()
        .same_site(SameSite::Lax)
        .max_age(Duration::from_secs(ttl));
    if settings.secure_cookies {
        cookie.secure()
    } else {
        cookie
    }
}

fn login_removal_cookie(settings: &Settings) -> Cookie {
    let cookie = Cookie::removal(LOGIN_COOKIE)
        .path(LOGIN_COOKIE_PATH)
        .http_only()
        .same_site(SameSite::Lax);
    if settings.secure_cookies {
        cookie.secure()
    } else {
        cookie
    }
}

/// Start signing in.
///
/// Returns the identity provider's authorization URL; send the browser there.
/// The provider redirects back to the web app with a `code` and `state` for
/// `createSession`. The response also sets the httpOnly `stacks_login`
/// cookie, which ties the sign-in to this browser: `createSession` must be
/// called from the same one.
#[kynos::post("/auth/oidc/authorizations", operation_id = "createOidcAuthorization")]
async fn create_oidc_authorization(
    Inject(auth): Inject<Arc<AuthService>>,
    Inject(settings): Inject<Arc<Settings>>,
) -> Result<WithHeaders<Json<OidcAuthorization>, SetCookieHeaders>, BeginSignInError> {
    let started = auth.begin_login().await.map_err(|e| match e {
        AuthError::IdentityProvider(why) => {
            tracing::warn!(reason = %why, "identity provider unavailable");
            BeginSignInError::IdentityProviderUnavailable
        }
        AuthError::Db(e) => Internal::from(e).into(),
        other => Internal::log(other).into(),
    })?;
    Ok(WithHeaders::new(
        Json(OidcAuthorization {
            authorization_url: started.url,
        }),
        SetCookieHeaders {
            cookies: vec![login_cookie(started.state, &settings)],
        },
    ))
}

/// Finish signing in.
///
/// Verifies the `code` and `state` the identity provider redirected back
/// with, creates the account on a first sign-in, and sets the httpOnly
/// session cookie. The `state` must be the one this browser's
/// `stacks_login` cookie carries, which `createOidcAuthorization` set; the
/// cookie is cleared once the sign-in completes.
#[kynos::post("/sessions", operation_id = "createSession")]
async fn create_session(
    current: Link<get_current_session>,
    Headers(client): Headers<ClientHeaders>,
    Cookies(login): Cookies<LoginCookie>,
    forwarded: Forwarded,
    Inject(auth): Inject<Arc<AuthService>>,
    Inject(settings): Inject<Arc<Settings>>,
    Json(body): Json<NewSession>,
) -> Result<WithHeaders<Created<Json<CurrentSession>>, SetCookieHeaders>, SignInError> {
    let meta = SessionMeta {
        user_agent: client.user_agent,
        ip: forwarded.client().map(|ip| ip.to_string()),
    };
    let started = auth
        .complete_login(&body.code, &body.state, login.state.as_deref(), meta)
        .await?;
    let cookie = session_cookie(started.token, auth.session_ttl(), &settings);
    let body = CurrentSession {
        session: Session::from_model(&started.session, Some(started.session.id)),
        user: User::from(&started.user),
    };
    Ok(WithHeaders::new(
        Created::at(current.uri(()), Json(body)),
        SetCookieHeaders {
            cookies: vec![cookie, login_removal_cookie(&settings)],
        },
    ))
}

/// Read the session this request was made with.
#[kynos::get("/sessions/current", operation_id = "getCurrentSession")]
async fn get_current_session(Auth(caller): SessionOnly) -> Json<CurrentSession> {
    Json(CurrentSession {
        session: Session::from_model(&caller.session, Some(caller.session.id)),
        user: User::from(&caller.user),
    })
}

/// Sign out: end this session and clear its cookie.
#[kynos::delete("/sessions/current", operation_id = "deleteCurrentSession")]
async fn delete_current_session(
    Auth(caller): SessionOnly,
    Inject(auth): Inject<Arc<AuthService>>,
    Inject(settings): Inject<Arc<Settings>>,
) -> Result<WithHeaders<NoContent, SetCookieHeaders>, Internal> {
    auth.end_session(caller.user.id, caller.session.id)
        .await
        .map_err(Internal::log)?;
    Ok(WithHeaders::new(
        NoContent,
        SetCookieHeaders {
            cookies: vec![removal_cookie(&settings)],
        },
    ))
}

/// What listing sessions can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// List the caller's signed-in browsers, newest first.
#[kynos::get("/sessions", operation_id = "listSessions")]
async fn list_sessions(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(auth): Inject<Arc<AuthService>>,
) -> Result<Json<Page<Session>>, ListError> {
    let caller = Caller::from(caller);
    let current = caller.session_id();
    let sessions = auth
        .list_sessions(caller.id())
        .await
        .map_err(Internal::log)?;
    let page = page::by_id_desc(sessions, |s| s.id, &params)?;
    Ok(Json(page.map(|s| Session::from_model(&s, current))))
}

/// What ending another session can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum EndSessionError {
    #[error("no session of yours has that id")]
    #[problem(status = 404, title = "Session not found")]
    SessionNotFound,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// Sign a browser out.
///
/// Ends one of the caller's sessions, such as a lost device's. Ending the
/// current session this way leaves its cookie in place but useless; use
/// `deleteCurrentSession` to sign out here.
#[kynos::delete("/sessions/{id}", operation_id = "deleteSession")]
async fn delete_session(
    Auth(caller): SessionOnly,
    Path(path): Path<SessionPath>,
    Inject(auth): Inject<Arc<AuthService>>,
) -> Result<NoContent, EndSessionError> {
    let ended = auth
        .end_session(caller.user.id, path.id)
        .await
        .map_err(Internal::log)?;
    if ended {
        Ok(NoContent)
    } else {
        Err(EndSessionError::SessionNotFound)
    }
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Sessions>().mount(kynos::routes![
        create_oidc_authorization,
        create_session,
        get_current_session,
        delete_current_session,
        list_sessions,
        delete_session,
    ])
}
