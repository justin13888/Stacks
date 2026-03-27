use sea_orm::DatabaseConnection;

use crate::jobs::JobError;

/// Payload for a BeamSync job.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BeamSyncPayload {
    pub server_id: String,
}

/// Handle a BeamSync job: fetch library from the Beam server and persist
/// updated `server_library_item` records.
pub async fn handle(db: &DatabaseConnection, payload: BeamSyncPayload) -> Result<(), JobError> {
    let _ = (db, payload);
    todo!()
}
