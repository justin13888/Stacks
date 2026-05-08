use std::sync::Arc;

use cameo::CameoClient;
use sea_orm::DatabaseConnection;

use super::{JobError, JobStatus, JobType, handlers, repo};

/// Polls the `jobs` table for pending work and dispatches it to the
/// appropriate handler. Started once at API startup; calls `tick()` in a
/// loop with a small sleep in between.
pub struct JobRunner {
    pub db: DatabaseConnection,
    pub cameo: Arc<CameoClient>,
}

impl JobRunner {
    pub fn new(db: DatabaseConnection, cameo: Arc<CameoClient>) -> Self {
        Self { db, cameo }
    }

    /// Run one polling pass: fetch up to `batch_size` pending jobs, execute
    /// each, and update its status. Errors from individual jobs are logged
    /// and stored on the row but do not abort the tick.
    pub async fn tick(&self) -> Result<(), JobError> {
        let pending = repo::find_pending_jobs(&self.db, 16).await?;
        for job in pending {
            let id = job.id.clone();
            // Mark running (best-effort — if this fails we still attempt the work).
            let _ = repo::update_job_status(&self.db, &id, JobStatus::Running, None).await;
            match self.run_job_inner(&job).await {
                Ok(()) => {
                    let _ =
                        repo::update_job_status(&self.db, &id, JobStatus::Completed, None).await;
                }
                Err(e) => {
                    let attempts = job.attempts + 1;
                    let exhausted = attempts >= job.max_attempts;
                    let new_status = if exhausted {
                        JobStatus::Failed
                    } else {
                        JobStatus::Pending
                    };
                    tracing::warn!(job_id = %id, attempts, error = %e, "job failed");
                    let _ =
                        repo::update_job_status(&self.db, &id, new_status, Some(e.to_string()))
                            .await;
                }
            }
        }
        Ok(())
    }

    async fn run_job_inner(&self, job: &crate::entity::job::Model) -> Result<(), JobError> {
        let kind = JobType::parse(&job.job_type)
            .ok_or_else(|| JobError::UnknownJobType(job.job_type.clone()))?;
        let payload = job
            .payload
            .clone()
            .ok_or_else(|| JobError::ExecutionFailed("job has no payload".into()))?;

        match kind {
            JobType::BeamSync => {
                let p: handlers::beam_sync::BeamSyncPayload = serde_json::from_value(payload)
                    .map_err(|e| JobError::ExecutionFailed(format!("invalid payload: {e}")))?;
                handlers::beam_sync::handle(&self.db, p).await
            }
            JobType::Import => {
                let p: handlers::import::ImportPayload = serde_json::from_value(payload)
                    .map_err(|e| JobError::ExecutionFailed(format!("invalid payload: {e}")))?;
                handlers::import::handle(&self.db, &self.cameo, p).await
            }
            JobType::Notification => {
                let p: handlers::notification::NotificationPayload =
                    serde_json::from_value(payload)
                        .map_err(|e| JobError::ExecutionFailed(format!("invalid payload: {e}")))?;
                handlers::notification::handle(&self.db, &self.cameo, p).await
            }
        }
    }
}
