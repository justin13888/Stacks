use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("token expired")]
    TokenExpired,
    #[error("invalid token")]
    InvalidToken,
    #[error("insufficient scope: required {required}")]
    InsufficientScope { required: String },
    #[error("account deactivated")]
    Deactivated,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("OIDC error: {0}")]
    OidcError(String),
    #[error("user not yet provisioned")]
    UserNotProvisioned,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
