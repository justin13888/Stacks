use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::JobType;
use crate::import_export::ImportPipeline;
use crate::jobs::{JobContext, JobError, repo};
use crate::notifications::service::NotificationEvent;

/// Payload for an `import` job. The uploaded file itself lives on the
/// `imports` row ([`crate::import_export::import::pipeline::submit`]).
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportPayload {
    pub import_id: Uuid,
}

/// Run the import, then queue its completion notification.
pub async fn handle(ctx: &JobContext, payload: ImportPayload) -> Result<(), JobError> {
    let pipeline = ImportPipeline::new(ctx.db.clone(), ctx.cameo.clone());
    let report = pipeline
        .run(payload.import_id)
        .await
        .map_err(|e| JobError::ExecutionFailed(format!("import run: {e}")))?;
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
