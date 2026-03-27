use thiserror::Error;

#[derive(Debug, Error)]
pub enum WatchStateError {
    #[error("invalid status: {0}")]
    InvalidStatus(String),
    #[error("score must be 0–100")]
    InvalidScore,
    #[error("watch status not found")]
    NotFound,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
