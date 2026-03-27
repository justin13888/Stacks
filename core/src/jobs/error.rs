use thiserror::Error;

#[derive(Debug, Error)]
pub enum JobError {
    #[error("job not found")]
    NotFound,
    #[error("unknown job type: {0}")]
    UnknownJobType(String),
    #[error("job execution failed: {0}")]
    ExecutionFailed(String),
    #[error("max attempts exceeded")]
    MaxAttemptsExceeded,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
