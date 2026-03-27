use sea_orm::DatabaseConnection;

use crate::import_export::models::ConflictStrategy;
use crate::jobs::JobError;

/// Payload for an Import job.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ImportPayload {
    pub user_id: String,
    pub source: String,
    pub data_base64: String,
    pub conflict_strategy: ConflictStrategy,
}

/// Handle an Import job: decode the data, parse entries, and run the import pipeline.
pub async fn handle(db: &DatabaseConnection, payload: ImportPayload) -> Result<(), JobError> {
    let _ = (db, payload);
    todo!()
}
