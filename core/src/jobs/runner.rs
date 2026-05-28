use std::time::Duration;

use super::handlers;
use super::repo;
use super::{JobContext, JobError};

/// Polls the `jobs` table and dispatches due jobs to their handlers.
///
/// The runner is single-threaded by design — each tick claims up to
/// [`BATCH`] pending jobs and runs them sequentially. Failures are caught and
/// either rescheduled with exponential backoff or marked permanently failed
/// once `max_attempts` is exhausted.
pub struct JobRunner {
    ctx: JobContext,
}

/// Number of pending jobs claimed per polling tick.
const BATCH: u64 = 16;

/// Cap on the per-failure backoff (1 hour).
const MAX_BACKOFF_SECS: i64 = 3600;

/// Base backoff in seconds — doubled on each subsequent attempt.
const BASE_BACKOFF_SECS: i64 = 30;

impl JobRunner {
    pub fn new(ctx: JobContext) -> Self {
        Self { ctx }
    }

    /// Run forever: poll for due jobs every `interval`, execute each.
    pub async fn run(self, interval: Duration) {
        loop {
            if let Err(e) = self.run_once().await {
                tracing::error!(error = %e, "job runner tick failed");
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// One polling tick: execute every due pending job, up to [`BATCH`].
    /// Returns the number of jobs dispatched.
    pub async fn run_once(&self) -> Result<u64, JobError> {
        let jobs = repo::find_pending_jobs(&self.ctx.db, BATCH).await?;
        let mut ran = 0u64;
        for job in jobs {
            if let Err(e) = self.run_job(&job.id).await {
                tracing::error!(job_id = %job.id, error = %e, "job failed");
            }
            ran += 1;
        }
        Ok(ran)
    }

    /// Execute a single job by id: mark it running, dispatch by job type, then
    /// mark completed / rescheduled / failed depending on the outcome.
    pub async fn run_job(&self, job_id: &str) -> Result<(), JobError> {
        let job = repo::mark_running(&self.ctx.db, job_id).await?;
        let job_type = repo::JobType::parse(&job.job_type)
            .ok_or_else(|| JobError::UnknownJobType(job.job_type.clone()))?;
        let payload = job.payload.clone().unwrap_or(serde_json::Value::Null);

        let outcome = match job_type {
            repo::JobType::BeamSync => dispatch_beam_sync(&self.ctx, payload).await,
            repo::JobType::Import => dispatch_import(&self.ctx, payload).await,
            repo::JobType::Notification => dispatch_notification(&self.ctx, payload).await,
        };

        match outcome {
            Ok(()) => {
                repo::mark_completed(&self.ctx.db, job_id).await?;
                Ok(())
            }
            Err(e) => {
                let msg = e.to_string();
                if job.attempts >= job.max_attempts {
                    repo::mark_failed(&self.ctx.db, job_id, msg).await?;
                } else {
                    let backoff = backoff_for(job.attempts);
                    repo::reschedule(&self.ctx.db, job_id, backoff, Some(msg)).await?;
                }
                Err(e)
            }
        }
    }
}

/// Exponential backoff capped at [`MAX_BACKOFF_SECS`]: 30s, 60s, 120s, …
fn backoff_for(attempts: i32) -> i64 {
    let exp = attempts.saturating_sub(1).clamp(0, 7) as u32;
    (BASE_BACKOFF_SECS.saturating_mul(1i64 << exp)).min(MAX_BACKOFF_SECS)
}

async fn dispatch_beam_sync(ctx: &JobContext, payload: serde_json::Value) -> Result<(), JobError> {
    let p: handlers::beam_sync::BeamSyncPayload = serde_json::from_value(payload)
        .map_err(|e| JobError::ExecutionFailed(format!("beam_sync payload: {e}")))?;
    handlers::beam_sync::handle(ctx, p).await
}

async fn dispatch_import(ctx: &JobContext, payload: serde_json::Value) -> Result<(), JobError> {
    let p: handlers::import::ImportPayload = serde_json::from_value(payload)
        .map_err(|e| JobError::ExecutionFailed(format!("import payload: {e}")))?;
    handlers::import::handle(ctx, p).await
}

async fn dispatch_notification(
    ctx: &JobContext,
    payload: serde_json::Value,
) -> Result<(), JobError> {
    let p: handlers::notification::NotificationPayload = serde_json::from_value(payload)
        .map_err(|e| JobError::ExecutionFailed(format!("notification payload: {e}")))?;
    handlers::notification::handle(ctx, p).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_caps() {
        assert_eq!(backoff_for(1), 30);
        assert_eq!(backoff_for(2), 60);
        assert_eq!(backoff_for(3), 120);
        assert_eq!(backoff_for(7), 30 * 64);
        // 30 * 128 = 3840 would exceed the 3600 cap.
        assert_eq!(backoff_for(8), MAX_BACKOFF_SECS);
        assert_eq!(backoff_for(50), MAX_BACKOFF_SECS);
    }
}
