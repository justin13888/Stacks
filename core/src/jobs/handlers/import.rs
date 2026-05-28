use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::import_export::ImportPipeline;
use crate::import_export::import::{mal, tmdb};
use crate::import_export::models::{ConflictStrategy, ImportEntry};
use crate::jobs::{JobContext, JobError};

/// Payload for an `import` job. `data_base64` is the source export bytes,
/// b64-encoded so the payload survives a JSON column.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportPayload {
    pub user_id: String,
    /// `mal_xml` | `mal_json` | `tmdb_json`
    pub source: String,
    pub data_base64: String,
    pub conflict_strategy: ConflictStrategy,
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
    Ok(())
}
