use std::time::Duration;

use crate::entity::enums::JobStatus;
use crate::entity::job;

use super::handlers;
use super::repo::{self, JobType};
use super::{JobContext, JobError};

/// Polls the `jobs` table and dispatches due jobs to their handlers.
///
/// Each tick claims up to [`BATCH`] due jobs with `FOR UPDATE SKIP LOCKED`
/// (so concurrent runners never share a job) and runs them sequentially.
/// Failures are rescheduled with exponential backoff, or marked permanently
/// failed once `max_attempts` is exhausted. A [`JobError::Deferred`] run that
/// made progress comes back after the base backoff without spending an
/// attempt.
pub struct JobRunner {
    ctx: JobContext,
}

/// Number of jobs claimed per polling tick.
const BATCH: u64 = 16;

/// Cap on the per-failure backoff (1 hour).
const MAX_BACKOFF_SECS: i64 = 3600;

/// Base backoff in seconds — doubled on each subsequent attempt.
const BASE_BACKOFF_SECS: i64 = 30;

/// A job still `running` this long after it was claimed is presumed
/// abandoned by a worker that stopped (a restart or crash mid-run) and is
/// reclaimed. It is far above any legitimate run — a large import paced by
/// the provider rate limits is the longest — so a live job is never run
/// twice in practice.
pub const STALE_RUNNING_AFTER_SECS: i64 = 6 * 60 * 60;

/// How often the runner queues a `metadata_refresh` batch.
pub const REFRESH_EVERY: Duration = Duration::from_secs(60 * 60);

impl JobRunner {
    pub fn new(ctx: JobContext) -> Self {
        Self { ctx }
    }

    /// Run forever: poll for due jobs every `interval`, execute each, and
    /// queue a `metadata_refresh` every [`REFRESH_EVERY`].
    pub async fn run(self, interval: Duration) {
        let mut next_refresh = tokio::time::Instant::now();
        loop {
            // The tick runs first: it reclaims a refresh a previous process
            // left `running`, so the schedule below is not blocked by it.
            if let Err(e) = self.run_once().await {
                tracing::error!(error = %e, "job runner tick failed");
            }
            if tokio::time::Instant::now() >= next_refresh {
                match self.schedule_refresh().await {
                    Ok(_) => next_refresh += REFRESH_EVERY,
                    Err(e) => tracing::error!(error = %e, "failed to queue metadata refresh"),
                }
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// Queue a periodic `metadata_refresh` unless one is already queued.
    /// Returns whether a job was queued.
    pub async fn schedule_refresh(&self) -> Result<bool, JobError> {
        let payload =
            serde_json::to_value(handlers::metadata_refresh::MetadataRefreshPayload::default())
                .map_err(|e| JobError::ExecutionFailed(format!("refresh payload: {e}")))?;
        Ok(
            repo::enqueue_unless_queued(&self.ctx.db, JobType::MetadataRefresh, Some(payload), 1)
                .await?
                .is_some(),
        )
    }

    /// One polling tick: reclaim jobs abandoned mid-run (see
    /// [`STALE_RUNNING_AFTER_SECS`]), then claim and execute due jobs, up to
    /// [`BATCH`]. Returns the number of jobs dispatched.
    pub async fn run_once(&self) -> Result<u64, JobError> {
        let reclaimed = repo::reclaim_stale_jobs(&self.ctx.db, STALE_RUNNING_AFTER_SECS).await?;
        if !reclaimed.is_empty() {
            tracing::warn!(
                reclaimed = reclaimed.len(),
                "reclaimed jobs abandoned mid-run"
            );
        }
        for job in reclaimed
            .iter()
            .filter(|j| j.status == JobStatus::Failed && j.job_type == JobType::Import)
        {
            handlers::import::give_up(
                &self.ctx,
                job.payload.as_ref(),
                job.error.as_deref().unwrap_or("abandoned"),
            )
            .await;
        }
        let jobs = repo::claim_due_jobs(&self.ctx.db, BATCH).await?;
        let ran = jobs.len() as u64;
        for job in jobs {
            let id = job.id;
            if let Err(e) = self.execute(job).await {
                tracing::error!(job_id = %id, error = %e, "job failed");
            }
        }
        Ok(ran)
    }

    /// Execute one claimed job, then mark it completed / rescheduled / failed.
    async fn execute(&self, job: job::Model) -> Result<(), JobError> {
        let payload = job.payload.clone().unwrap_or(serde_json::Value::Null);
        let outcome = match job.job_type {
            JobType::Import => dispatch_import(&self.ctx, payload).await,
            JobType::Notification => dispatch_notification(&self.ctx, payload).await,
            JobType::MetadataRefresh => dispatch_metadata_refresh(&self.ctx, payload).await,
        };
        match outcome {
            Ok(()) => {
                repo::mark_completed(&self.ctx.db, job.id).await?;
                Ok(())
            }
            // Progress: back soon, and this run is not counted as an attempt.
            Err(
                e @ JobError::Deferred {
                    progressed: true, ..
                },
            ) => {
                repo::defer(&self.ctx.db, job.id, BASE_BACKOFF_SECS, e.to_string()).await?;
                Err(e)
            }
            Err(e) => {
                let msg = e.to_string();
                if job.attempts >= job.max_attempts {
                    repo::mark_failed(&self.ctx.db, job.id, msg.clone()).await?;
                    if job.job_type == JobType::Import {
                        handlers::import::give_up(&self.ctx, job.payload.as_ref(), &msg).await;
                    }
                } else {
                    let backoff = backoff_for(job.attempts);
                    repo::reschedule(&self.ctx.db, job.id, backoff, Some(msg)).await?;
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

async fn dispatch_metadata_refresh(
    ctx: &JobContext,
    payload: serde_json::Value,
) -> Result<(), JobError> {
    // The periodic job carries an empty object; `null` is treated the same.
    let p: handlers::metadata_refresh::MetadataRefreshPayload = if payload.is_null() {
        Default::default()
    } else {
        serde_json::from_value(payload)
            .map_err(|e| JobError::ExecutionFailed(format!("metadata_refresh payload: {e}")))?
    };
    handlers::metadata_refresh::handle(ctx, p).await
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
