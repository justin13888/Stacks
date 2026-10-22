use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::import_export::ImportPipeline;
use crate::import_export::import::{mal, tmdb};
use crate::import_export::models::{ConflictStrategy, ImportEntry};
use crate::jobs::{JobContext, JobError, repo};
use crate::notifications::service::NotificationEvent;

/// Payload for an `import` job. `data_base64` is the source export bytes,
/// b64-encoded so the payload survives a JSON column.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportPayload {
    pub user_id: String,
    /// `mal_xml` | `mal_json` | `tmdb_json`
    pub source: String,
    pub data_base64: String,
    pub conflict_strategy: ConflictStrategy,
    /// The originating job id, echoed into the completion notification.
    #[serde(default)]
    pub import_id: Option<String>,
}

/// Decode + parse the upload, then run it through [`ImportPipeline::run`].
pub async fn handle(ctx: &JobContext, payload: ImportPayload) -> Result<(), JobError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.data_base64.as_bytes())
        .map_err(|e| JobError::ExecutionFailed(format!("import payload base64: {e}")))?;
    let entries: Vec<ImportEntry> = match payload.source.as_str() {
        "mal_xml" => mal::parse_mal_xml(&bytes),
        "mal_json" => mal::parse_mal_json(&bytes),
        "tmdb_json" => tmdb::parse_tmdb_json(&bytes),
        other => {
            return Err(JobError::ExecutionFailed(format!(
                "unsupported import source: {other}"
            )));
        }
    }
    .map_err(|e| JobError::ExecutionFailed(format!("parse {}: {e}", payload.source)))?;

    let pipeline = ImportPipeline::new(ctx.db.clone(), ctx.cameo.clone());
    let report = pipeline
        .run(&payload.user_id, entries, payload.conflict_strategy)
        .await
        .map_err(|e| JobError::ExecutionFailed(format!("import run: {e}")))?;
    tracing::info!(
        user_id = %payload.user_id,
        total = report.total,
        succeeded = report.succeeded,
        skipped = report.skipped,
        failed = report.failed,
        "import job complete"
    );

    // Tell the user their import finished.
    //
    // Nothing anywhere previously enqueued a notification job, so the entire
    // notification subsystem — channels, preferences, rendering, delivery — was
    // reachable through the API but could never actually fire. Enqueuing rather
    // than dispatching inline keeps delivery retries under the JobRunner, and a
    // failure to enqueue must not fail an import that already succeeded.
    let event = NotificationEvent::ImportCompleted {
        import_id: payload.import_id.clone().unwrap_or_default(),
        total: report.total as u32,
        succeeded: report.succeeded as u32,
        failed: report.failed as u32,
    };
    let notification = repo::build_job(
        repo::JobType::Notification,
        Some(serde_json::json!({
            "user_id": payload.user_id,
            "event": event,
        })),
        3,
    );
    if let Err(e) = repo::enqueue_job(&ctx.db, notification).await {
        tracing::warn!(
            user_id = %payload.user_id,
            error = %e,
            "import finished but the completion notification could not be enqueued"
        );
    }

    Ok(())
}
