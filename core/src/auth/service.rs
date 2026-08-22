use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use ulid::Ulid;

use crate::config::models::{AuthConfig, OidcConfig};
use crate::entity::{api_token, user};
use crate::user::repo as user_repo;

use super::{
    api_token::{
        Scope, generate as generate_api_token, has_scope, lookup_hash as api_token_lookup,
        verify_token,
    },
    error::AuthError,
    jwt::{decode_access_token, encode_access_token},
    oidc::{OidcUserInfo, authorization_url, exchange_code},
    refresh::{generate as generate_refresh, hash_token as hash_refresh, save as save_refresh},
    repo,
};

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
    const OIDC_SESSION_TTL_SECS: i64 = 600;

    /// Build the OIDC authorization redirect URL and record the pending request.
    ///
    /// The generated `state` and `nonce` are persisted server-side rather than
    /// handed to the client. That is what makes the callback verifiable: the
    /// client cannot forge a nonce it never sees, and an unknown `state` is
    /// rejected outright.
    pub async fn oidc_login_url(&self) -> Result<super::oidc::AuthorizationParams, AuthError> {
        let params = authorization_url(
            self.oidc_config.server_base(),
            &self.oidc_config.issuer,
            &self.oidc_config.client_id,
            &self.oidc_config.redirect_uri,
        )
        .await?;

        // Abandoned logins would otherwise accumulate forever.
        if let Err(e) = repo::purge_expired_oidc_sessions(&self.db).await {
            tracing::warn!(error = %e, "failed to purge expired oidc sessions");
        }

        repo::insert_oidc_session(
            &self.db,
            &params.state,
            &params.nonce,
            Utc::now() + chrono::Duration::seconds(Self::OIDC_SESSION_TTL_SECS),
        )
        .await?;

        Ok(params)
    }

    /// Handle OIDC callback: verify `state`, exchange code, provision, issue tokens.
    pub async fn oidc_callback(&self, code: &str, state: &str) -> Result<TokenPair, AuthError> {
        // Consuming the row here is both the CSRF check (an attacker cannot
        // produce a `state` we issued) and the replay check (single-use).
        let session = repo::take_oidc_session(&self.db, state)
            .await?
            .ok_or_else(|| AuthError::OidcError("unknown or already-used state".into()))?;

        if session.expires_at < Utc::now() {
            return Err(AuthError::OidcError("authorization request expired".into()));
        }

        let user_info = exchange_code(
            self.oidc_config.server_base(),
            &self.oidc_config.client_id,
            &self.oidc_config.client_secret,
            &self.oidc_config.redirect_uri,
            code,
            &session.nonce,
        )
        .await?;

        let user = self.provision_user(&user_info).await?;
        self.issue_tokens(&user.id, &user.role).await
    }

    /// Whether this email is configured for the `admin` role.
    fn is_admin_email(&self, email: Option<&str>) -> bool {
        let Some(email) = email else { return false };
        let email = email.to_ascii_lowercase();
        self.auth_config.admin_emails.contains(&email)
    }

    /// Provision user from OIDC info if not already present.
    ///
    /// Also reconciles the admin allowlist on every login, so adding an address
    /// to `WW_ADMIN_EMAILS` promotes an existing account on its next sign-in
    /// rather than requiring a manual `UPDATE users SET role = 'admin'`.
    async fn provision_user(&self, info: &OidcUserInfo) -> Result<user::Model, AuthError> {
        let should_be_admin = self.is_admin_email(info.email.as_deref());

        if let Some(existing) = user_repo::find_by_oidc_sub(&self.db, &info.sub).await? {
            if should_be_admin && existing.role != "admin" {
                tracing::info!(user_id = %existing.id, "promoting user to admin via WW_ADMIN_EMAILS");
                return crate::user::service::UserService::new(self.db.clone())
                    .update_role(&existing.id, "admin".to_string())
                    .await
                    .map_err(|e| AuthError::OidcError(format!("role promotion failed: {e}")));
            }
            return Ok(existing);
        }

        // `&info.sub[..8]` panicked on subs shorter than 8 bytes and on any sub
        // whose 8th byte fell inside a multi-byte char.
        let username = info.preferred_username.clone().unwrap_or_else(|| {
            let suffix: String = info.sub.chars().take(8).collect();
            format!("user_{suffix}")
        });

        let new_user = user::ActiveModel {
            id: Set(Ulid::new().to_string()),
            oidc_sub: Set(info.sub.clone()),
            username: Set(username),
            email: Set(info.email.clone()),
            role: Set(if should_be_admin { "admin" } else { "user" }.to_string()),
            settings: Set(serde_json::json!({})),
            created_at: Set(Utc::now().into()),
            updated_at: Set(Utc::now().into()),
        };

        user_repo::insert(&self.db, new_user)
            .await
            .map_err(|_| AuthError::OidcError("user provisioning failed".into()))
    }

    /// Validate refresh token and issue a new access + refresh token pair.
    pub async fn refresh(&self, refresh_token_plaintext: &str) -> Result<TokenPair, AuthError> {
        let token_hash = hash_refresh(refresh_token_plaintext);
        let token = repo::find_refresh_token(&self.db, &token_hash)
            .await?
            .ok_or(AuthError::InvalidToken)?;

        if token.expires_at < Utc::now() {
            return Err(AuthError::TokenExpired);
        }

        let user = user_repo::find_by_id(&self.db, &token.user_id)
            .await?
            .ok_or(AuthError::InvalidToken)?;

        // Rotate: delete old token
        repo::delete_refresh_token(&self.db, &token.id).await?;

        self.issue_tokens(&user.id, &user.role).await
    }

    /// Revoke all refresh tokens for a user (logout).
    pub async fn logout(&self, user_id: &str) -> Result<(), AuthError> {
        repo::delete_user_refresh_tokens(&self.db, user_id).await
    }

    /// Create a new scoped API token for a user.
    pub async fn create_api_token(
        &self,
        user_id: &str,
        label: &str,
        scopes: Vec<String>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(api_token::Model, String), AuthError> {
        let (plaintext, token_hash, token_lookup) = generate_api_token();
        let model = api_token::ActiveModel {
            id: Set(Ulid::new().to_string()),
            user_id: Set(user_id.to_string()),
            token_hash: Set(token_hash),
            token_lookup: Set(Some(token_lookup)),
            label: Set(label.to_string()),
            scopes: Set(serde_json::to_value(&scopes).unwrap()),
            expires_at: Set(expires_at.map(Into::into)),
            created_at: Set(Utc::now().into()),
        };
        let saved = repo::insert_api_token(&self.db, model).await?;
        Ok((saved, plaintext))
    }

    /// Validate a raw API token and return the associated user if valid.
    pub async fn validate_api_token(
        &self,
        plaintext: &str,
        required_scope: Option<&Scope>,
    ) -> Result<user::Model, AuthError> {
        // Fast path: one indexed lookup on the deterministic SHA-256. This used
        // to load every token row for every user and argon2-verify each in turn
        // — ~50-100ms per stored token on every authenticated request.
        let mut candidates = Vec::new();
        if let Some(found) =
            repo::find_api_token_by_lookup(&self.db, &api_token_lookup(plaintext)).await?
        {
            candidates.push(found);
        } else {
            // Tokens issued before m018 have no lookup hash; scan just those.
            candidates = repo::find_legacy_api_tokens(&self.db).await?;
        }

        for token in candidates {
            if let Some(exp) = token.expires_at
                && exp < Utc::now()
            {
                continue;
            }
            if verify_token(plaintext, &token.token_hash) {
                if let Some(scope) = required_scope {
                    let scopes: Vec<String> =
                        serde_json::from_value(token.scopes.clone()).unwrap_or_default();
                    if !has_scope(&scopes, scope) {
                        return Err(AuthError::InsufficientScope {
                            required: scope.as_str().to_string(),
                        });
                    }
                }
                return user_repo::find_by_id(&self.db, &token.user_id)
                    .await?
                    .ok_or(AuthError::InvalidToken);
            }
        }
        Err(AuthError::InvalidToken)
    }

    pub async fn delete_api_token(&self, id: &str, user_id: &str) -> Result<bool, AuthError> {
        repo::delete_api_token(&self.db, id, user_id).await
    }

    pub async fn list_api_tokens(&self, user_id: &str) -> Result<Vec<api_token::Model>, AuthError> {
        repo::list_api_tokens(&self.db, user_id).await
    }

    /// Validate a JWT access token and return claims.
    pub fn validate_access_token(&self, token: &str) -> Result<super::jwt::Claims, AuthError> {
        decode_access_token(token, &self.auth_config.secret_key)
    }

    async fn issue_tokens(&self, user_id: &str, role: &str) -> Result<TokenPair, AuthError> {
        let access_token = encode_access_token(
            user_id,
            role,
            &self.auth_config.secret_key,
            self.auth_config.access_token_ttl_secs,
        )?;

        let refresh = generate_refresh(user_id, self.auth_config.refresh_token_ttl_secs);
        save_refresh(&self.db, refresh.model).await?;

        Ok(TokenPair {
            access_token,
            refresh_token: refresh.plaintext,
            expires_in: self.auth_config.access_token_ttl_secs,
        })
    }
}

#[derive(Debug)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}
