//! Who is calling, and what they may do.
//!
//! Two credentials reach the API (docs/adr/0004-auth-sessions.md):
//!
//! - the web app's **session**, an opaque token in the httpOnly
//!   `stacks_session` cookie, minted by `POST /api/v1/sessions` after an
//!   OpenID Connect sign-in;
//! - a script's **API token** (`stk_…`), sent as a bearer credential, which
//!   carries scopes: `read`, `write` and `admin`.
//!
//! Each guard is a type, so what an operation demands is both enforced and
//! declared by its first argument:
//!
//! | Guard | Accepts | `security` |
//! | --- | --- | --- |
//! | [`Reader`] | a session, or a token with `read` | `[{SessionCookie: []}, {ApiToken: [read]}]` |
//! | [`Writer`] | a session, or a token with `write` | `[{SessionCookie: []}, {ApiToken: [write]}]` |
//! | [`Administrator`] | an admin's session, or an admin's token with `admin` | `[{SessionCookie: [admin]}, {ApiToken: [admin]}]` |
//! | [`SessionOnly`] | a session | `[{SessionCookie: []}]` |
//!
//! The scopes ride on the *scheme*, as its default scopes, rather than on a
//! `Scoped` guard: kynos 0.4's `Scoped<S, R>` takes one scheme, and these
//! guards accept either of two (getkono/kynos#1000). One component key
//! registered by several scheme types with different defaults is how the
//! description says "a session, or a token holding `write`".

use kynos::error::rejection::AuthRejection;
use kynos::prelude::*;
use kynos::security::carrier::{ApiKey, BearerToken};
use kynos::security::requirement::{AnyOf, Either2};
use kynos::security::{Authenticates, Authenticator, Scope as KynosScope};
use stacks_core::entity::enums::Scope;
use stacks_core::entity::{api_token, session, user};
use uuid::Uuid;

use crate::app::App;

/// The cookie the session travels in.
pub const SESSION_COOKIE: &str = "stacks_session";

macro_rules! problem_type {
    ($slug:literal) => {
        concat!(
            "https://github.com/justin13888/Stacks/blob/master/docs/adr/0006-error-model.md#",
            $slug
        )
    };
}

/// The 403 for a credential whose account an administrator deactivated.
pub const ACCOUNT_DEACTIVATED: &str = problem_type!("account-deactivated");
/// The 403 for a token lacking a scope the operation demands, or a
/// non-administrator reaching an administrative operation.
pub const INSUFFICIENT_SCOPE: &str = problem_type!("insufficient-scope");
/// The 403 for a request whose credential could not be checked because the
/// credential store (PostgreSQL) failed.
///
/// A 503 would be the honest status, and kynos 0.4.0 lets an authenticator
/// answer only 401 or 403 (getkono/kynos#993, fixed on kynos `master` after
/// 0.4.0; move to its 503 when a release carries it). A 401 would tell the
/// web app the session is gone and sign the user out during an outage, so the
/// refusal is a 403 with a type of its own that a client can tell apart and
/// retry.
pub const CREDENTIAL_STORE_UNAVAILABLE: &str = problem_type!("credential-store-unavailable");

/// One grant a credential may carry, as kynos sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    Read,
    Write,
    Admin,
}

impl KynosScope for Grant {
    fn name(&self) -> &'static str {
        self.scope().as_str()
    }
}

impl Grant {
    fn scope(self) -> Scope {
        match self {
            Self::Read => Scope::Read,
            Self::Write => Scope::Write,
            Self::Admin => Scope::Admin,
        }
    }
}

/// A caller who presented a session.
#[derive(Clone, Debug)]
pub struct SessionCaller {
    pub user: user::Model,
    pub session: session::Model,
}

/// A caller who presented an API token.
#[derive(Clone, Debug)]
pub struct TokenCaller {
    pub user: user::Model,
    pub token: api_token::Model,
    pub scopes: Vec<Scope>,
}

/// How the caller authenticated.
#[derive(Clone, Debug)]
pub enum Via {
    Session(session::Model),
    Token(api_token::Model),
}

/// A verified caller: the account, and the credential that proved it.
#[derive(Clone, Debug)]
pub struct Caller {
    pub user: user::Model,
    pub via: Via,
}

impl Caller {
    pub fn id(&self) -> Uuid {
        self.user.id
    }

    /// The session the request was made with, if it was made with one.
    pub fn session_id(&self) -> Option<Uuid> {
        match &self.via {
            Via::Session(session) => Some(session.id),
            Via::Token(_) => None,
        }
    }
}

impl From<SessionCaller> for Caller {
    fn from(c: SessionCaller) -> Self {
        Self {
            user: c.user,
            via: Via::Session(c.session),
        }
    }
}

impl From<TokenCaller> for Caller {
    fn from(c: TokenCaller) -> Self {
        Self {
            user: c.user,
            via: Via::Token(c.token),
        }
    }
}

impl From<Either2<SessionCaller, TokenCaller>> for Caller {
    fn from(either: Either2<SessionCaller, TokenCaller>) -> Self {
        match either {
            Either2::First(session) => session.into(),
            Either2::Second(token) => token.into(),
        }
    }
}

// Each component key is registered by two or three scheme types below. The
// registry merges them only while their descriptions are identical, and
// reports a conflict otherwise, so the router's validation test catches a
// description edited in one place and not the others.

/// A session, demanding nothing beyond being signed in.
#[derive(SecurityScheme)]
#[security(api_key(in = "cookie", name = "stacks_session"))]
#[security(name = "SessionCookie", credential = SessionCaller, scope = Grant)]
#[security(
    description = "The web app's session: an opaque token in an httpOnly cookie, set by `createSession`."
)]
pub struct Session;

/// A session whose account is an administrator.
#[derive(SecurityScheme)]
#[security(api_key(in = "cookie", name = "stacks_session"))]
#[security(name = "SessionCookie", credential = SessionCaller, scope = Grant, scopes(Grant::Admin))]
#[security(
    description = "The web app's session: an opaque token in an httpOnly cookie, set by `createSession`."
)]
pub struct AdminSession;

/// An API token holding `read`.
#[derive(SecurityScheme)]
#[security(bearer)]
#[security(name = "ApiToken", credential = TokenCaller, scope = Grant, scopes(Grant::Read))]
#[security(
    description = "A personal API token (`stk_…`), created with `createApiToken`. Each operation names the scope it needs."
)]
pub struct ReadToken;

/// An API token holding `write`.
#[derive(SecurityScheme)]
#[security(bearer)]
#[security(name = "ApiToken", credential = TokenCaller, scope = Grant, scopes(Grant::Write))]
#[security(
    description = "A personal API token (`stk_…`), created with `createApiToken`. Each operation names the scope it needs."
)]
pub struct WriteToken;

/// An administrator's API token holding `admin`.
#[derive(SecurityScheme)]
#[security(bearer)]
#[security(name = "ApiToken", credential = TokenCaller, scope = Grant, scopes(Grant::Admin))]
#[security(
    description = "A personal API token (`stk_…`), created with `createApiToken`. Each operation names the scope it needs."
)]
pub struct AdminToken;

/// Reads the caller's own data.
pub type Reader = Auth<AnyOf<(Session, ReadToken)>>;
/// Changes the caller's own data.
pub type Writer = Auth<AnyOf<(Session, WriteToken)>>;
/// Administers the deployment.
pub type Administrator = Auth<AnyOf<(AdminSession, AdminToken)>>;
/// Manages credentials, which only a signed-in browser may do: a stolen token
/// cannot mint more tokens or end its owner's sessions.
pub type SessionOnly = Auth<Session>;

/// Verifies both kinds of credential against the database.
pub struct Verifier;

/// Whether an account holds a grant through its role. `admin` is a role;
/// `read` and `write` belong to every account.
fn role_allows(user: &user::Model, grant: Grant) -> bool {
    grant != Grant::Admin || user.is_admin()
}

/// A genuine credential whose account is deactivated is refused rather than
/// unknown: the 403 tells the client that signing in again will not help.
fn admit(user: &user::Model) -> Result<(), AuthRejection> {
    if user.is_active() {
        Ok(())
    } else {
        Err(AuthRejection::forbidden_as(ACCOUNT_DEACTIVATED))
    }
}

fn store_unavailable(e: sea_orm::DbErr) -> AuthRejection {
    tracing::error!(error = %e, "credential lookup failed");
    AuthRejection::forbidden_as(CREDENTIAL_STORE_UNAVAILABLE)
}

impl Verifier {
    async fn session(presented: &ApiKey, context: &App) -> Result<SessionCaller, AuthRejection> {
        let found = context
            .auth
            .authenticate_session(presented.as_str())
            .await
            .map_err(store_unavailable)?
            .ok_or_else(AuthRejection::unauthenticated)?;
        admit(&found.user)?;
        Ok(SessionCaller {
            user: found.user,
            session: found.credential,
        })
    }

    async fn token(presented: &BearerToken, context: &App) -> Result<TokenCaller, AuthRejection> {
        let found = context
            .auth
            .authenticate_token(presented.as_str())
            .await
            .map_err(store_unavailable)?
            .ok_or_else(AuthRejection::unauthenticated)?;
        admit(&found.user)?;
        let scopes = found
            .credential
            .scopes
            .iter()
            .filter_map(|s| Scope::parse(s))
            .collect();
        Ok(TokenCaller {
            user: found.user,
            token: found.credential,
            scopes,
        })
    }
}

fn refuse_unless(allowed: bool) -> Result<(), AuthRejection> {
    if allowed {
        Ok(())
    } else {
        Err(AuthRejection::forbidden_as(INSUFFICIENT_SCOPE))
    }
}

macro_rules! cookie_scheme {
    ($($scheme:ty),+) => {$(
        impl Authenticator<$scheme, App> for Verifier {
            async fn authenticate(
                &self,
                presented: ApiKey,
                context: &App,
            ) -> Result<SessionCaller, AuthRejection> {
                Self::session(&presented, context).await
            }

            /// A session holds `read` and `write` by being one; `admin` needs
            /// an administrator behind it.
            async fn authorize(
                &self,
                caller: &SessionCaller,
                scopes: &[Grant],
                _context: &App,
            ) -> Result<(), AuthRejection> {
                refuse_unless(scopes.iter().all(|g| role_allows(&caller.user, *g)))
            }
        }

        impl Authenticates<$scheme> for App {
            type Authenticator = Verifier;

            fn authenticator(&self) -> &Verifier {
                &Verifier
            }
        }
    )+};
}

macro_rules! bearer_scheme {
    ($($scheme:ty),+) => {$(
        impl Authenticator<$scheme, App> for Verifier {
            async fn authenticate(
                &self,
                presented: BearerToken,
                context: &App,
            ) -> Result<TokenCaller, AuthRejection> {
                Self::token(&presented, context).await
            }

            /// A token holds what it was granted, and never more than its
            /// owner's role: an `admin` token stops working when its owner is
            /// demoted.
            async fn authorize(
                &self,
                caller: &TokenCaller,
                scopes: &[Grant],
                _context: &App,
            ) -> Result<(), AuthRejection> {
                refuse_unless(scopes.iter().all(|g| {
                    caller.scopes.contains(&g.scope()) && role_allows(&caller.user, *g)
                }))
            }
        }

        impl Authenticates<$scheme> for App {
            type Authenticator = Verifier;

            fn authenticator(&self) -> &Verifier {
                &Verifier
            }
        }
    )+};
}

cookie_scheme!(Session, AdminSession);
bearer_scheme!(ReadToken, WriteToken, AdminToken);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_types_share_the_base() {
        for uri in [
            ACCOUNT_DEACTIVATED,
            INSUFFICIENT_SCOPE,
            CREDENTIAL_STORE_UNAVAILABLE,
        ] {
            assert!(uri.starts_with(crate::problem::BASE), "{uri}");
        }
    }
}
