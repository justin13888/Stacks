use thiserror::Error;

use crate::entity::enums::JobStatus;

#[derive(Debug, Error)]
pub enum JobError {
    #[error("job not found")]
    NotFound,
    #[error("unknown job type: {0}")]
    UnknownJobType(String),
    #[error("job execution failed: {0}")]
    ExecutionFailed(String),
    /// Only a failed job can be retried: a pending or running one is still
    /// queued, and a completed one has nothing left to do.
    #[error("a {0} job cannot be retried; only a failed one can")]
    NotRetryable(JobStatus),
    #[error("max attempts exceeded")]
    MaxAttemptsExceeded,
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}
