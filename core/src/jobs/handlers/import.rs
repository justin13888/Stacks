use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use cameo::CameoClient;
use sea_orm::DatabaseConnection;

use crate::import_export::ImportPipeline;
use crate::import_export::import::{mal, tmdb};
use crate::import_export::models::{ConflictStrategy, ImportEntry};
use crate::jobs::JobError;

/// Payload for an Import job.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ImportPayload {
    pub user_id: String,
    /// Source format identifier: `tmdb_json`, `mal_xml`, `mal_json`, `stacks_json`.
    pub source: String,
    /// Base64-encoded raw bytes of the upload.
    pub data_base64: String,
    pub conflict_strategy: ConflictStrategy,
}

/// Handle an Import job: decode the data, parse entries, and run the import pipeline.
pub async fn handle(
    db: &DatabaseConnection,
    cameo: &Arc<CameoClient>,
    payload: ImportPayload,
) -> Result<(), JobError> {
    let data = B64
        .decode(payload.data_base64.as_bytes())
        .map_err(|e| JobError::ExecutionFailed(format!("base64: {e}")))?;

    let entries: Vec<ImportEntry> = match payload.source.as_str() {
        "tmdb_json" | "tmdb" => tmdb::parse_tmdb_json(&data)
            .map_err(|e| JobError::ExecutionFailed(e.to_string()))?,
        "mal_xml" | "mal" => mal::parse_mal_xml(&data)
            .map_err(|e| JobError::ExecutionFailed(e.to_string()))?,
        "mal_json" => mal::parse_mal_json(&data)
            .map_err(|e| JobError::ExecutionFailed(e.to_string()))?,
        "stacks_json" => serde_json::from_slice(&data)
            .map_err(|e| JobError::ExecutionFailed(format!("stacks_json: {e}")))?,
        other => {
            return Err(JobError::ExecutionFailed(format!(
                "unsupported import source: {other}"
            )));
        }
    };

    let pipeline = ImportPipeline::new(db.clone(), cameo.clone());
    let report = pipeline
        .run(&payload.user_id, entries, payload.conflict_strategy)
        .await
        .map_err(|e| JobError::ExecutionFailed(e.to_string()))?;
    tracing::info!(
        user_id = %payload.user_id,
        total = report.total,
        succeeded = report.succeeded,
        failed = report.failed,
        skipped = report.skipped,
        "import job completed"
    );
    Ok(())
}
