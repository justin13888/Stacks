use thiserror::Error;

#[derive(Debug, Error)]
pub enum UserError {
    #[error("user not found")]
    NotFound,
    #[error("display name must not be empty")]
    EmptyDisplayName,
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
    #[error("a user must be deactivated before it can be purged")]
    PurgeRequiresDeactivation,
    #[error("purge confirmation does not match the user")]
    PurgeNotConfirmed,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
