use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{ImportStatus, JobType};
use crate::import_export::ImportPipeline;
use crate::jobs::{JobContext, JobError, repo};
use crate::notifications::service::NotificationEvent;

/// Payload for an `import` job. The uploaded file itself lives on the
/// `imports` row ([`crate::import_export::import::pipeline::submit`]).
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportPayload {
    pub import_id: Uuid,
}

/// The runner gave up on an `import` job (its attempts are spent, or a
/// worker abandoned it on its last one): fail the import it was running, so
/// it does not sit `pending` saying it will retry.
pub async fn give_up(ctx: &JobContext, payload: Option<&serde_json::Value>, why: &str) {
    let Some(p) = payload
        .cloned()
        .and_then(|v| serde_json::from_value::<ImportPayload>(v).ok())
    else {
        return;
    };
    if let Err(e) = crate::import_export::import::pipeline::give_up(&ctx.db, p.import_id, why).await
    {
        tracing::error!(import_id = %p.import_id, error = %e, "could not fail an abandoned import");
    }
}

/// Run the import, then queue its completion notification.
///
/// An import with rows still waiting for the metadata provider is not done:
/// the job answers [`JobError::Deferred`], so the runner reschedules it, and
/// the next run picks up exactly the waiting rows. A run that settled rows
/// does not spend an attempt. An import whose stored file no longer parses
/// is failed for good, with no completion notification.
pub async fn handle(ctx: &JobContext, payload: ImportPayload) -> Result<(), JobError> {
    let pipeline = ImportPipeline::new(ctx.db.clone(), ctx.metadata.clone());
    let run = pipeline
        .run(payload.import_id)
        .await
        .map_err(|e| JobError::ExecutionFailed(format!("import run: {e}")))?;
    let report = run.counts;
    match run.status {
        ImportStatus::Failed => {
            tracing::warn!(
                import_id = %payload.import_id,
                "import failed: its stored file no longer parses"
            );
            return Ok(());
        }
        ImportStatus::Pending => {
            tracing::warn!(
                import_id = %payload.import_id,
                waiting = report.pending_retry,
                settled = run.settled,
                "import deferred: the metadata provider is unavailable"
            );
            return Err(JobError::Deferred {
                reason: format!(
                    "{} rows wait for the metadata provider",
                    report.pending_retry
                ),
                progressed: run.settled > 0,
            });
        }
        ImportStatus::Completed | ImportStatus::Running => {}
    }
    tracing::info!(
        import_id = %payload.import_id,
        total = report.total,
        applied = report.applied,
        skipped = report.skipped,
        unmatched = report.unmatched,
        failed = report.failed,
        "import job complete"
    );

    let user_id =
        match crate::import_export::import::pipeline::owner(&ctx.db, payload.import_id).await {
            Ok(Some(u)) => u,
            _ => return Ok(()),
        };
    // Enqueued rather than dispatched inline so delivery retries stay with the
    // JobRunner; failing to enqueue must not fail an import that succeeded.
    let event = NotificationEvent::ImportCompleted {
        import_id: payload.import_id.to_string(),
        total: report.total,
        succeeded: report.applied,
        failed: report.failed + report.unmatched,
    };
    let notification = repo::build_job(
        JobType::Notification,
        Some(serde_json::json!({ "user_id": user_id, "event": event })),
        3,
    );
    if let Err(e) = repo::enqueue_job(&ctx.db, notification).await {
        tracing::warn!(
            import_id = %payload.import_id,
            error = %e,
            "import finished but the completion notification could not be enqueued"
        );
    }
    Ok(())
}
