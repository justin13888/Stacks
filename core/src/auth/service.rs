//! Sign-in, sessions and API tokens.
//!
//! A browser signs in through OpenID Connect and gets an opaque session
//! token; a script uses a scoped API token. Both are 256-bit random secrets of
//! which only the SHA-256 is stored, so a database read discloses no usable
//! credential. There are no signed tokens and nothing to refresh: revoking a
//! session or a token is deleting or flagging one row, and takes effect on the
//! next request.

use std::sync::Arc;

use chrono::Utc;
use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::config::models::{AuthConfig, OidcConfig};
use crate::entity::enums::{Role, Scope};
use crate::entity::{api_token, oidc_login_state, session, user};
use crate::user::repo as user_repo;

use super::api_token::{generate as generate_api_token, lookup_hash};
use super::error::AuthError;
use super::oidc::{OidcProvider, VerifiedIdentity};
use super::{repo, session as session_token};

/// Request metadata recorded on a session, for the user's session list.
#[derive(Debug, Clone, Default)]
pub struct SessionMeta {
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

/// A session just started. `token` is the plaintext the client keeps; it is
/// never stored and cannot be recovered.
#[derive(Debug)]
pub struct NewSession {
    pub token: String,
    pub session: session::Model,
    pub user: user::Model,
}

/// A credential that authenticated, with the account it belongs to.
#[derive(Debug, Clone)]
pub struct Authenticated<T> {
    pub credential: T,
    pub user: user::Model,
}

/// How long a session's `last_seen_at` may lag before a request refreshes it.
/// Refreshing on every request would turn every read into a write.
const SEEN_GRANULARITY: chrono::Duration = chrono::Duration::minutes(5);

/// How long a pending authorization request stays valid.
pub const LOGIN_STATE_TTL: chrono::Duration = chrono::Duration::minutes(10);

/// A sign-in just begun.
#[derive(Debug, Clone)]
pub struct LoginStart {
    /// Where to send the browser.
    pub url: String,
    /// The request's `state`. The browser that began the sign-in keeps it
    /// (the API sets it as a cookie) and must present it again to finish.
    pub state: String,
}

pub struct AuthService {
    db: DatabaseConnection,
    config: AuthConfig,
    oidc: Arc<OidcProvider>,
}

impl AuthService {
    pub fn new(db: DatabaseConnection, config: AuthConfig, oidc: Arc<OidcProvider>) -> Self {
        Self { db, config, oidc }
    }

    // ------------------------------------------------------------ sign-in

    /// Begin a sign-in: record the pending request (state, nonce and PKCE
    /// verifier stay server-side) and return the URL to send the browser to,
    /// with the `state` the browser must keep.
    pub async fn begin_login(&self) -> Result<LoginStart, AuthError> {
        let request = self.oidc.authorization_request().await?;
        let start = LoginStart {
            url: request.url,
            state: request.state.clone(),
        };

        // Abandoned sign-ins would otherwise accumulate forever.
        if let Err(e) = repo::purge_expired_login_states(&self.db).await {
            tracing::warn!(error = %e, "failed to purge expired login states");
        }
        repo::insert_login_state(
            &self.db,
            oidc_login_state::ActiveModel {
                id: Set(Uuid::now_v7()),
                state: Set(request.state),
                nonce: Set(request.nonce),
                pkce_verifier: Set(request.pkce_verifier),
                redirect_to: Set(None),
                expires_at: Set((Utc::now() + LOGIN_STATE_TTL).into()),
                created_at: NotSet,
            },
        )
        .await?;
        Ok(start)
    }

    /// Finish a sign-in: consume the pending request, exchange the code,
    /// verify the id_token, provision the account and start a session.
    ///
    /// `browser_state` is the `state` the finishing browser kept from
    /// [`begin_login`](Self::begin_login). It must match `state`: a `code`
    /// and `state` that another browser obtained (an attacker's own sign-in,
    /// passed to a victim in a callback link) would otherwise sign the victim
    /// into the attacker's account.
    pub async fn complete_login(
        &self,
        code: &str,
        state: &str,
        browser_state: Option<&str>,
        meta: SessionMeta,
    ) -> Result<NewSession, AuthError> {
        // Compared as digests, so the comparison's timing says nothing about
        // the browser's value.
        if browser_state.map(session_token::hash_token) != Some(session_token::hash_token(state)) {
            return Err(AuthError::LoginRejected(
                "the sign-in was not started in this browser".into(),
            ));
        }
        // Consuming the row is the replay check (it matches once), and only a
        // state Stacks issued matches.
        let pending = repo::take_login_state(&self.db, state)
            .await?
            .ok_or_else(|| AuthError::LoginRejected("unknown or already-used state".into()))?;
        if pending.expires_at < Utc::now() {
            return Err(AuthError::LoginRejected(
                "the sign-in request expired".into(),
            ));
        }
        let identity = self
            .oidc
            .exchange(code, &pending.nonce, &pending.pkce_verifier)
            .await?;
        let user = self.provision_user(&identity).await?;
        if !user.is_active() {
            return Err(AuthError::Deactivated);
        }
        self.start_session(user, meta).await
    }

    /// Whether the allowlist names this identity's address. Only an address
    /// the provider verified counts: an unverified one may be anyone's.
    fn is_admin_identity(&self, identity: &VerifiedIdentity) -> bool {
        identity.email_verified
            && identity
                .email
                .as_deref()
                .is_some_and(|e| self.config.admin_emails.contains(&e.to_ascii_lowercase()))
    }

    /// Find or create the account for an identity.
    ///
    /// The admin allowlist is reconciled on every sign-in, so adding an
    /// address to `WW_ADMIN_EMAILS` promotes an existing account the next time
    /// it signs in. Removing one never demotes: that is an admin's decision.
    async fn provision_user(&self, identity: &VerifiedIdentity) -> Result<user::Model, AuthError> {
        let issuer = self.oidc_issuer();
        let should_be_admin = self.is_admin_identity(identity);

        if let Some(existing) =
            user_repo::find_by_identity(&self.db, issuer, &identity.subject).await?
        {
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

        let new_user = user::ActiveModel {
            id: Set(Uuid::now_v7()),
            oidc_issuer: Set(issuer.to_owned()),
            oidc_subject: Set(identity.subject.clone()),
            email: Set(identity.email.clone()),
            display_name: Set(display_name(identity)),
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
            // Two first sign-ins racing: the other one created the row.
            Err(e) if crate::db::is_unique_violation(&e) => {
                user_repo::find_by_identity(&self.db, issuer, &identity.subject)
                    .await?
                    .ok_or(AuthError::Db(e))
            }
            Err(e) => Err(e.into()),
        }
    }

    fn oidc_issuer(&self) -> &str {
        self.oidc.issuer()
    }

    async fn start_session(
        &self,
        user: user::Model,
        meta: SessionMeta,
    ) -> Result<NewSession, AuthError> {
        if let Err(e) = repo::end_expired_sessions(&self.db).await {
            tracing::warn!(error = %e, "failed to clean up expired sessions");
        }
        let (token, hash) = session_token::generate();
        let ttl = chrono::Duration::seconds(
            i64::try_from(self.config.session_ttl_secs).unwrap_or(i64::MAX),
        );
        let session = repo::insert_session(
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
        Ok(NewSession {
            token,
            session,
            user,
        })
    }

    /// How long a new session lives.
    pub fn session_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.session_ttl_secs)
    }

    // ----------------------------------------------------------- sessions

    /// The live session a token names, with its account; `None` when the
    /// token is unknown or the session has expired. A deactivated account is
    /// returned as such: refusing it is the caller's decision.
    pub async fn authenticate_session(
        &self,
        token: &str,
    ) -> Result<Option<Authenticated<session::Model>>, sea_orm::DbErr> {
        let Some(found) =
            repo::find_session_by_hash(&self.db, &session_token::hash_token(token)).await?
        else {
            return Ok(None);
        };
        let now = Utc::now();
        if found.expires_at < now {
            repo::end_session(&self.db, found.id).await?;
            return Ok(None);
        }
        let Some(user) = user_repo::find_by_id(&self.db, found.user_id).await? else {
            return Ok(None);
        };
        if found.last_seen_at < now - SEEN_GRANULARITY {
            // Bookkeeping only; a failure here must not refuse the request.
            if let Err(e) = repo::touch_session(&self.db, found.id).await {
                tracing::warn!(session_id = %found.id, error = %e, "failed to record session use");
            }
        }
        Ok(Some(Authenticated {
            credential: found,
            user,
        }))
    }

    /// A user's live sessions, newest first.
    pub async fn list_sessions(&self, user_id: Uuid) -> Result<Vec<session::Model>, AuthError> {
        Ok(repo::list_sessions(&self.db, user_id).await?)
    }

    /// End one of a user's sessions. Returns whether it existed.
    pub async fn end_session(&self, user_id: Uuid, session_id: Uuid) -> Result<bool, AuthError> {
        Ok(repo::end_user_session(&self.db, user_id, session_id).await?)
    }

    /// End every session of a user (deactivation, sign-out everywhere).
    pub async fn end_all_sessions(&self, user_id: Uuid) -> Result<(), AuthError> {
        Ok(repo::end_user_sessions(&self.db, user_id).await?)
    }

    // --------------------------------------------------------- API tokens

    /// Create a scoped API token; the plaintext is returned exactly once.
    ///
    /// The `admin` scope is only granted to an administrator: a token never
    /// carries more authority than its owner.
    pub async fn create_api_token(
        &self,
        owner: &user::Model,
        name: &str,
        scopes: &[Scope],
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<(api_token::Model, String), AuthError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AuthError::InvalidRequest(
                "token name must not be empty".into(),
            ));
        }
        if scopes.is_empty() {
            return Err(AuthError::InvalidRequest(
                "at least one scope is required".into(),
            ));
        }
        if scopes.contains(&Scope::Admin) && !owner.is_admin() {
            return Err(AuthError::ScopeNotPermitted(
                "only an administrator can grant the admin scope".into(),
            ));
        }
        if expires_at.is_some_and(|at| at <= Utc::now()) {
            return Err(AuthError::InvalidRequest(
                "the expiry must be in the future".into(),
            ));
        }
        let mut granted: Vec<String> = Vec::with_capacity(scopes.len());
        for scope in scopes {
            let s = scope.as_str().to_owned();
            if !granted.contains(&s) {
                granted.push(s);
            }
        }
        let token = generate_api_token();
        let saved = repo::insert_api_token(
            &self.db,
            api_token::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(owner.id),
                name: Set(name.to_owned()),
                token_prefix: Set(token.prefix),
                lookup_hash: Set(token.lookup_hash),
                scopes: Set(granted),
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

    /// The live token a plaintext names, with its account; `None` when it is
    /// unknown, revoked or expired. Scopes are the caller's to check.
    pub async fn authenticate_token(
        &self,
        plaintext: &str,
    ) -> Result<Option<Authenticated<api_token::Model>>, sea_orm::DbErr> {
        let Some(token) = repo::find_api_token_by_hash(&self.db, &lookup_hash(plaintext)).await?
        else {
            return Ok(None);
        };
        let now = Utc::now();
        if token.revoked_at.is_some() || token.expires_at.is_some_and(|exp| exp < now) {
            return Ok(None);
        }
        let Some(user) = user_repo::find_by_id(&self.db, token.user_id).await? else {
            return Ok(None);
        };
        let stale = token
            .last_used_at
            .is_none_or(|used| used < now - SEEN_GRANULARITY);
        if stale && let Err(e) = repo::touch_api_token(&self.db, token.id).await {
            tracing::warn!(token_id = %token.id, error = %e, "failed to record token use");
        }
        Ok(Some(Authenticated {
            credential: token,
            user,
        }))
    }

    /// Revoke an API token. The row is kept with `revoked_at` set.
    pub async fn revoke_api_token(&self, user_id: Uuid, id: Uuid) -> Result<bool, AuthError> {
        Ok(repo::revoke_api_token(&self.db, id, user_id).await?)
    }

    /// A user's live tokens, newest first.
    pub async fn list_api_tokens(&self, user_id: Uuid) -> Result<Vec<api_token::Model>, AuthError> {
        Ok(repo::list_api_tokens(&self.db, user_id).await?)
    }
}

/// A provider-supplied handle, then the email's local part, and only then a
/// slice of the opaque subject.
fn display_name(identity: &VerifiedIdentity) -> String {
    identity
        .preferred_username
        .clone()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| {
            identity
                .email
                .as_deref()
                .and_then(|e| e.split('@').next())
                .filter(|local| !local.is_empty())
                .map(String::from)
        })
        .unwrap_or_else(|| {
            let suffix: String = identity.subject.chars().take(8).collect();
            format!("user_{suffix}")
        })
}

/// The OIDC configuration a deployment's [`AuthService`] signs in against.
pub fn oidc_provider(config: &OidcConfig) -> Result<OidcProvider, AuthError> {
    let http = super::oidc::OidcHttp::new(std::time::Duration::from_secs(10))?;
    Ok(OidcProvider::new(config.clone(), http))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(username: Option<&str>, email: Option<&str>) -> VerifiedIdentity {
        VerifiedIdentity {
            subject: "0123456789abcdef".into(),
            email: email.map(String::from),
            email_verified: true,
            preferred_username: username.map(String::from),
        }
    }

    #[test]
    fn display_name_prefers_the_handle_then_the_email() {
        assert_eq!(display_name(&identity(Some("ada"), Some("x@y.z"))), "ada");
        assert_eq!(
            display_name(&identity(Some("  "), Some("grace@y.z"))),
            "grace"
        );
        assert_eq!(display_name(&identity(None, None)), "user_01234567");
    }
}
