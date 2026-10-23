use chrono::Utc;
use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::config::models::{AuthConfig, OidcConfig};
use crate::entity::enums::{Role, Scope};
use crate::entity::{api_token, oidc_login_state, session, user};
use crate::user::repo as user_repo;

use super::api_token::{generate as generate_api_token, has_scope, lookup_hash, parse_scopes};
use super::error::AuthError;
use super::jwt::{Claims, decode_access_token, encode_access_token};
use super::oidc::{AuthorizationParams, OidcUserInfo, authorization_url, exchange_code};
use super::{repo, session as session_token};

/// Request metadata recorded on a session, for the user's session list.
#[derive(Debug, Clone, Default)]
pub struct SessionMeta {
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

/// What a successful sign-in or refresh hands the client.
#[derive(Debug)]
pub struct TokenPair {
    /// Short-lived JWT for `Authorization: Bearer`.
    pub access_token: String,
    /// The opaque session token, exchanged for a new pair on refresh.
    pub refresh_token: String,
    pub expires_in: u64,
}

pub struct AuthService {
    db: DatabaseConnection,
    auth_config: AuthConfig,
    oidc_config: OidcConfig,
}

impl AuthService {
    pub fn new(db: DatabaseConnection, auth_config: AuthConfig, oidc_config: OidcConfig) -> Self {
        Self {
            db,
            auth_config,
            oidc_config,
        }
    }

    /// How long a pending authorization request stays valid.
    const LOGIN_STATE_TTL_SECS: i64 = 600;

    /// Build the OIDC authorization redirect URL and record the pending
    /// request (state, nonce and PKCE verifier) server-side.
    pub async fn oidc_login_url(&self) -> Result<AuthorizationParams, AuthError> {
        let params = authorization_url(
            self.oidc_config.server_base(),
            &self.oidc_config.issuer,
            &self.oidc_config.client_id,
            &self.oidc_config.redirect_uri,
        )
        .await?;

        // Abandoned logins would otherwise accumulate forever.
        if let Err(e) = repo::purge_expired_login_states(&self.db).await {
            tracing::warn!(error = %e, "failed to purge expired login states");
        }

        repo::insert_login_state(
            &self.db,
            oidc_login_state::ActiveModel {
                id: Set(Uuid::now_v7()),
                state: Set(params.state.clone()),
                nonce: Set(params.nonce.clone()),
                pkce_verifier: Set(params.pkce_verifier.clone()),
                redirect_to: Set(None),
                expires_at: Set((Utc::now()
                    + chrono::Duration::seconds(Self::LOGIN_STATE_TTL_SECS))
                .into()),
                created_at: NotSet,
            },
        )
        .await?;
        Ok(params)
    }

    /// Handle the OIDC callback: verify `state`, exchange the code, provision
    /// the user, start a session.
    pub async fn oidc_callback(
        &self,
        code: &str,
        state: &str,
        meta: SessionMeta,
    ) -> Result<TokenPair, AuthError> {
        // Consuming the row is both the CSRF check (an attacker cannot produce
        // a `state` we issued) and the replay check (single-use).
        let pending = repo::take_login_state(&self.db, state)
            .await?
            .ok_or_else(|| AuthError::OidcError("unknown or already-used state".into()))?;
        if pending.expires_at < Utc::now() {
            return Err(AuthError::OidcError("authorization request expired".into()));
        }

        let info = exchange_code(
            self.oidc_config.server_base(),
            &self.oidc_config.client_id,
            &self.oidc_config.client_secret,
            &self.oidc_config.redirect_uri,
            code,
            &pending.nonce,
            &pending.pkce_verifier,
        )
        .await?;

        let user = self.provision_user(&info).await?;
        if !user.is_active() {
            return Err(AuthError::Deactivated);
        }
        self.start_session(&user, meta).await
    }

    fn is_admin_email(&self, email: Option<&str>) -> bool {
        let Some(email) = email else { return false };
        self.auth_config
            .admin_emails
            .contains(&email.to_ascii_lowercase())
    }

    /// Find or create the user for an OIDC identity.
    ///
    /// Also reconciles the admin allowlist on every login, so adding an
    /// address to `WW_ADMIN_EMAILS` promotes an existing account on its next
    /// sign-in.
    async fn provision_user(&self, info: &OidcUserInfo) -> Result<user::Model, AuthError> {
        let issuer = self.oidc_config.issuer.as_str();
        let should_be_admin = self.is_admin_email(info.email.as_deref());

        if let Some(existing) = user_repo::find_by_identity(&self.db, issuer, &info.sub).await? {
            if should_be_admin && existing.role != Role::Admin {
                tracing::info!(user_id = %existing.id, "promoting user to admin via WW_ADMIN_EMAILS");
                return Ok(user_repo::update(
                    &self.db,
                    user::ActiveModel {
                        id: Unchanged(existing.id),
                        role: Set(Role::Admin),
                        ..Default::default()
                    },
                )
                .await?);
            }
            return Ok(existing);
        }

        // Prefer a provider-supplied handle, then the email local part, and
        // only then a slice of the opaque subject.
        let display_name = info
            .preferred_username
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| {
                info.email
                    .as_deref()
                    .and_then(|e| e.split('@').next())
                    .filter(|local| !local.is_empty())
                    .map(String::from)
            })
            .unwrap_or_else(|| {
                let suffix: String = info.sub.chars().take(8).collect();
                format!("user_{suffix}")
            });

        let new_user = user::ActiveModel {
            id: Set(Uuid::now_v7()),
            oidc_issuer: Set(issuer.to_owned()),
            oidc_subject: Set(info.sub.clone()),
            email: Set(info.email.clone()),
            display_name: Set(display_name),
            role: Set(if should_be_admin {
                Role::Admin
            } else {
                Role::User
            }),
            settings: Set(serde_json::json!({})),
            deactivated_at: Set(None),
            created_at: NotSet,
            updated_at: NotSet,
        };
        match user_repo::insert(&self.db, new_user).await {
            Ok(u) => Ok(u),
            // Two first logins racing: the other one created the row.
            Err(e) if crate::db::is_unique_violation(&e) => {
                user_repo::find_by_identity(&self.db, issuer, &info.sub)
                    .await?
                    .ok_or(AuthError::UserNotProvisioned)
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn start_session(
        &self,
        user: &user::Model,
        meta: SessionMeta,
    ) -> Result<TokenPair, AuthError> {
        if let Err(e) = repo::end_expired_sessions(&self.db).await {
            tracing::warn!(error = %e, "failed to clean up expired sessions");
        }
        let (plaintext, hash) = session_token::generate();
        let ttl = chrono::Duration::seconds(self.auth_config.refresh_token_ttl_secs as i64);
        repo::insert_session(
            &self.db,
            session::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(user.id),
                token_hash: Set(hash),
                user_agent: Set(meta.user_agent),
                ip: Set(meta.ip),
                created_at: NotSet,
                last_seen_at: NotSet,
                expires_at: Set((Utc::now() + ttl).into()),
            },
        )
        .await?;
        let access_token = encode_access_token(
            &user.id.to_string(),
            user.role.as_str(),
            &self.auth_config.secret_key,
            self.auth_config.access_token_ttl_secs,
        )?;
        Ok(TokenPair {
            access_token,
            refresh_token: plaintext,
            expires_in: self.auth_config.access_token_ttl_secs,
        })
    }

    /// Exchange a session token for a fresh pair, rotating the session token.
    pub async fn refresh(&self, token: &str, meta: SessionMeta) -> Result<TokenPair, AuthError> {
        let found = repo::find_session_by_hash(&self.db, &session_token::hash_token(token))
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if found.expires_at < Utc::now() {
            repo::end_session(&self.db, found.id).await?;
            return Err(AuthError::TokenExpired);
        }
        let user = user_repo::find_by_id(&self.db, found.user_id)
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if !user.is_active() {
            return Err(AuthError::Deactivated);
        }
        // Rotate: the presented token is single-use.
        repo::end_session(&self.db, found.id).await?;
        self.start_session(&user, meta).await
    }

    /// End every session of a user.
    pub async fn logout(&self, user_id: Uuid) -> Result<(), AuthError> {
        Ok(repo::end_user_sessions(&self.db, user_id).await?)
    }

    /// Create a scoped API token; the plaintext is returned exactly once.
    pub async fn create_api_token(
        &self,
        user_id: Uuid,
        name: &str,
        scopes: Vec<String>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(api_token::Model, String), AuthError> {
        if name.trim().is_empty() {
            return Err(AuthError::InvalidRequest(
                "token name must not be empty".into(),
            ));
        }
        let scopes = parse_scopes(&scopes).map_err(AuthError::InvalidRequest)?;
        let token = generate_api_token();
        let saved = repo::insert_api_token(
            &self.db,
            api_token::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(user_id),
                name: Set(name.to_owned()),
                token_prefix: Set(token.prefix),
                lookup_hash: Set(token.lookup_hash),
                scopes: Set(scopes.iter().map(|s| s.as_str().to_owned()).collect()),
                expires_at: Set(expires_at.map(Into::into)),
                last_used_at: Set(None),
                revoked_at: Set(None),
                created_at: NotSet,
                updated_at: NotSet,
            },
        )
        .await?;
        Ok((saved, token.plaintext))
    }

    /// Authenticate a raw API token: one indexed lookup on its SHA-256.
    pub async fn validate_api_token(
        &self,
        plaintext: &str,
        required_scope: Option<Scope>,
    ) -> Result<user::Model, AuthError> {
        let token = repo::find_api_token_by_hash(&self.db, &lookup_hash(plaintext))
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if token.revoked_at.is_some() {
            return Err(AuthError::InvalidToken);
        }
        if token.expires_at.is_some_and(|exp| exp < Utc::now()) {
            return Err(AuthError::TokenExpired);
        }
        if let Some(scope) = required_scope
            && !has_scope(&token.scopes, scope)
        {
            return Err(AuthError::InsufficientScope {
                required: scope.as_str().to_owned(),
            });
        }
        let user = user_repo::find_by_id(&self.db, token.user_id)
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if !user.is_active() {
            return Err(AuthError::Deactivated);
        }
        if let Err(e) = repo::touch_api_token(&self.db, token.id).await {
            tracing::warn!(token_id = %token.id, error = %e, "failed to record token use");
        }
        Ok(user)
    }

    /// Revoke an API token. The row is kept with `revoked_at` set.
    pub async fn revoke_api_token(&self, id: Uuid, user_id: Uuid) -> Result<bool, AuthError> {
        Ok(repo::revoke_api_token(&self.db, id, user_id).await?)
    }

    pub async fn list_api_tokens(&self, user_id: Uuid) -> Result<Vec<api_token::Model>, AuthError> {
        Ok(repo::list_api_tokens(&self.db, user_id).await?)
    }

    /// Validate a JWT access token and return its claims.
    pub fn validate_access_token(&self, token: &str) -> Result<Claims, AuthError> {
        decode_access_token(token, &self.auth_config.secret_key)
    }

    /// The active user an access token belongs to.
    pub async fn user_for_access_token(&self, token: &str) -> Result<user::Model, AuthError> {
        let claims = self.validate_access_token(token)?;
        let id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError::InvalidToken)?;
        let user = user_repo::find_by_id(&self.db, id)
            .await?
            .ok_or(AuthError::InvalidToken)?;
        if !user.is_active() {
            return Err(AuthError::Deactivated);
        }
        Ok(user)
    }
}
