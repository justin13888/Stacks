use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::config::models::JobsConfig;
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
    retention: JobsConfig,
}

/// How [`JobRunner::execute`] left a claimed job.
enum Executed {
    /// The handler returned and its outcome is recorded.
    Finished,
    /// Shutdown abandoned the handler and the job went back to `pending`.
    Released,
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

/// How often the runner prunes finished jobs past their retention.
pub const PRUNE_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

impl JobRunner {
    /// A runner keeping finished jobs for [`JobsConfig::default`]'s
    /// retention; [`Self::with_retention`] sets the configured one.
    pub fn new(ctx: JobContext) -> Self {
        Self {
            ctx,
            retention: JobsConfig::default(),
        }
    }

    /// Keep finished jobs for `retention` instead of the default.
    pub fn with_retention(mut self, retention: JobsConfig) -> Self {
        self.retention = retention;
        self
    }

    /// Run until `shutdown` is cancelled: poll for due jobs every `interval`,
    /// execute each, queue a `metadata_refresh` every [`REFRESH_EVERY`], and
    /// [`Self::prune`] finished jobs at start and every [`PRUNE_EVERY`].
    ///
    /// Cancelling `shutdown` stops claiming, abandons the job in flight, and
    /// [`repo::release`]s it and every other job this tick claimed back to
    /// `pending` with its attempt returned, so the next process runs them at
    /// once rather than after [`STALE_RUNNING_AFTER_SECS`].
    pub async fn run(self, interval: Duration, shutdown: CancellationToken) {
        let mut next_refresh = tokio::time::Instant::now();
        let mut next_prune = next_refresh;
        while !shutdown.is_cancelled() {
            // The tick runs first: once a refresh a crashed process left
            // `running` is stale, the tick reclaims it, so the schedule
            // below is not blocked by it from then on.
            if let Err(e) = self.tick(&shutdown).await {
                tracing::error!(error = %e, "job runner tick failed");
            }
            if shutdown.is_cancelled() {
                break;
            }
            if tokio::time::Instant::now() >= next_refresh {
                match self.schedule_refresh().await {
                    Ok(_) => next_refresh += REFRESH_EVERY,
                    Err(e) => tracing::error!(error = %e, "failed to queue metadata refresh"),
                }
            }
            if tokio::time::Instant::now() >= next_prune {
                // A failed prune is retried on the next tick, like a refresh.
                match self.prune().await {
                    Ok(pruned) => {
                        if pruned > 0 {
                            tracing::info!(pruned, "pruned finished jobs past their retention");
                        }
                        next_prune += PRUNE_EVERY;
                    }
                    Err(e) => tracing::error!(error = %e, "failed to prune finished jobs"),
                }
            }
            tokio::select! {
                () = shutdown.cancelled() => {}
                () = tokio::time::sleep(interval) => {}
            }
        }
        tracing::info!("job runner stopped");
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

    /// Delete completed and failed jobs older than this runner's retention
    /// (see [`repo::prune_finished_jobs`]). Returns the number deleted.
    pub async fn prune(&self) -> Result<u64, JobError> {
        repo::prune_finished_jobs(&self.ctx.db, self.retention).await
    }

    /// One polling tick: reclaim jobs abandoned mid-run (see
    /// [`STALE_RUNNING_AFTER_SECS`]), then claim and execute due jobs, up to
    /// [`BATCH`]. Returns the number of jobs dispatched.
    pub async fn run_once(&self) -> Result<u64, JobError> {
        self.tick(&CancellationToken::new()).await
    }

    /// [`Self::run_once`], stopping when `shutdown` is cancelled: the job in
    /// flight is abandoned, and it and the claimed jobs not yet started are
    /// released back to `pending`. Released jobs are not counted.
    async fn tick(&self, shutdown: &CancellationToken) -> Result<u64, JobError> {
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
        if shutdown.is_cancelled() {
            return Ok(0);
        }
        let jobs = repo::claim_due_jobs(&self.ctx.db, BATCH).await?;
        let mut ran = 0;
        for job in jobs {
            let id = job.id;
            if shutdown.is_cancelled() {
                self.release(id).await;
                continue;
            }
            match self.execute(job, shutdown).await {
                Ok(Executed::Finished) => ran += 1,
                Ok(Executed::Released) => {}
                Err(e) => {
                    ran += 1;
                    tracing::error!(job_id = %id, error = %e, "job failed");
                }
            }
        }
        Ok(ran)
    }

    /// Return a claimed job to `pending` on shutdown. A failure is logged:
    /// the row stays `running` until [`STALE_RUNNING_AFTER_SECS`] reclaims it.
    async fn release(&self, id: uuid::Uuid) {
        match repo::release(&self.ctx.db, id).await {
            Ok(_) => tracing::info!(job_id = %id, "released job on shutdown"),
            Err(e) => {
                tracing::error!(job_id = %id, error = %e, "failed to release job on shutdown")
            }
        }
    }

    /// Execute one claimed job, then mark it completed / rescheduled / failed
    /// — or, when `shutdown` is cancelled mid-run, abandon the handler and
    /// release the job. Only the handler is abandoned: once it has returned,
    /// its outcome is recorded whatever `shutdown` does.
    async fn execute(
        &self,
        job: job::Model,
        shutdown: &CancellationToken,
    ) -> Result<Executed, JobError> {
        let payload = job.payload.clone().unwrap_or(serde_json::Value::Null);
        let dispatch = async {
            match job.job_type {
                JobType::Import => dispatch_import(&self.ctx, payload).await,
                JobType::Notification => dispatch_notification(&self.ctx, payload).await,
                JobType::MetadataRefresh => dispatch_metadata_refresh(&self.ctx, payload).await,
            }
        };
        let outcome = tokio::select! {
            outcome = dispatch => outcome,
            () = shutdown.cancelled() => {
                self.release(job.id).await;
                return Ok(Executed::Released);
            }
        };
        self.record(&job, outcome)
            .await
            .map(|()| Executed::Finished)
    }

    /// Record a handler's outcome on its job row.
    async fn record(
        &self,
        job: &job::Model,
        outcome: Result<(), JobError>,
    ) -> Result<(), JobError> {
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
