use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    /// A sign-in could not be completed and retrying it as-is will not help:
    /// an unknown, expired or already-used `state`, a code the identity
    /// provider refused, or an id_token that failed verification.
    #[error("sign-in failed: {0}")]
    LoginRejected(String),
    /// The identity provider could not be reached, or answered with
    /// something Stacks cannot use (discovery, key set or token endpoint).
    #[error("identity provider unavailable: {0}")]
    IdentityProvider(String),
    #[error("account deactivated")]
    Deactivated,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// A token asked for a scope its owner's role does not allow.
    #[error("scope not permitted: {0}")]
    ScopeNotPermitted(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
