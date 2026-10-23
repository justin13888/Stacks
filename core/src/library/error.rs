use thiserror::Error;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("not in the library")]
    NotFound,
    #[error("watch run not found")]
    RunNotFound,
    #[error("play not found")]
    PlayNotFound,
    #[error("play already voided")]
    AlreadyVoided,
    #[error("episodes can only be logged against a show")]
    NotAShow,
    #[error("a show's plays are of its episodes")]
    NotAMovie,
    #[error("score must be 0–100")]
    InvalidScore,
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
