use thiserror::Error;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("not in the library")]
    NotFound,
    /// The title is live in the library: an add that only creates, or a
    /// restore from the trash, leaves it as it is.
    #[error("already in the library")]
    AlreadyLive,
    #[error("watch run not found")]
    RunNotFound,
    #[error("play not found")]
    PlayNotFound,
    #[error("play already voided")]
    AlreadyVoided,
    #[error("play not voided")]
    NotVoided,
    #[error("episodes can only be logged against a show")]
    NotAShow,
    #[error("a show's plays are of its episodes")]
    NotAMovie,
    #[error("score must be 0–100")]
    InvalidScore,
    /// A run would finish before it started, or a rewatch would start
    /// before the run it follows finished.
    #[error("the run's dates are out of order")]
    RunDatesOutOfOrder,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

impl From<sea_orm::TransactionError<LibraryError>> for LibraryError {
    fn from(e: sea_orm::TransactionError<LibraryError>) -> Self {
        match e {
            sea_orm::TransactionError::Connection(e) => LibraryError::Db(e),
            sea_orm::TransactionError::Transaction(e) => e,
        }
    }
}
