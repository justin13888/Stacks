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
    /// An administrator tried to purge their own account.
    #[error("an administrator cannot purge their own account")]
    CannotPurgeOwnAccount,
    /// An administrator tried to demote or deactivate their own account,
    /// which could leave the deployment without an administrator who can
    /// undo it.
    #[error("an administrator cannot demote or deactivate their own account")]
    CannotChangeOwnAccount,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
