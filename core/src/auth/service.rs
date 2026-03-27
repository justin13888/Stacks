use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use ulid::Ulid;

use crate::config::models::{AuthConfig, OidcConfig};
use crate::entity::{api_token, user};
use crate::user::repo as user_repo;

use super::{
    api_token::{Scope, generate as generate_api_token, has_scope, verify_token},
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

    /// Build the OIDC authorization redirect URL.
    pub async fn oidc_login_url(&self) -> Result<super::oidc::AuthorizationParams, AuthError> {
        authorization_url(
            &self.oidc_config.issuer,
            &self.oidc_config.client_id,
            &self.oidc_config.redirect_uri,
        )
        .await
    }

    /// Handle OIDC callback: exchange code, provision user if new, issue tokens.
    pub async fn oidc_callback(
        &self,
        code: &str,
        state: &str,
        nonce: &str,
    ) -> Result<TokenPair, AuthError> {
        let user_info = exchange_code(
            &self.oidc_config.issuer,
            &self.oidc_config.client_id,
            &self.oidc_config.client_secret,
            &self.oidc_config.redirect_uri,
            code,
            state,
            nonce,
        )
        .await?;

        let user = self.provision_user(&user_info).await?;
        self.issue_tokens(&user.id, &user.role).await
    }

    /// Provision user from OIDC info if not already present.
    async fn provision_user(&self, info: &OidcUserInfo) -> Result<user::Model, AuthError> {
        if let Some(existing) = user_repo::find_by_oidc_sub(&self.db, &info.sub).await? {
            return Ok(existing);
        }

        let username = info
            .preferred_username
            .clone()
            .unwrap_or_else(|| format!("user_{}", &info.sub[..8]));

        let new_user = user::ActiveModel {
            id: Set(Ulid::new().to_string()),
            oidc_sub: Set(info.sub.clone()),
            username: Set(username),
            email: Set(info.email.clone()),
            role: Set("user".to_string()),
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
        let (plaintext, token_hash) = generate_api_token();
        let model = api_token::ActiveModel {
            id: Set(Ulid::new().to_string()),
            user_id: Set(user_id.to_string()),
            token_hash: Set(token_hash),
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
        let all_tokens = {
            // We can't search by hash directly since argon2 is not deterministic per lookup.
            // In production this should be indexed by a fast prefix or hmac.
            // For now: load tokens for all users and verify. TODO: optimize.
            use crate::entity::api_token::Entity;
            use sea_orm::EntityTrait;
            Entity::find().all(&self.db).await.map_err(AuthError::Db)?
        };

        for token in all_tokens {
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
