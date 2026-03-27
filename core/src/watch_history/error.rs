use thiserror::Error;

#[derive(Debug, Error)]
pub enum WatchHistoryError {
    #[error("watch history entry not found")]
    NotFound,
    #[error("invalid event type: {0}")]
    InvalidEvent(String),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
