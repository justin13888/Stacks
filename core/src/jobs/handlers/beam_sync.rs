use sea_orm::DatabaseConnection;

use crate::config::load as load_config;
use crate::jobs::JobError;
use crate::streaming::StreamingService;

/// Payload for a BeamSync job.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BeamSyncPayload {
    pub server_id: String,
}

/// Handle a BeamSync job: fetch the library from the configured streaming
/// server (Beam, Plex, etc.) and persist updated `server_library_item` records.
pub async fn handle(db: &DatabaseConnection, payload: BeamSyncPayload) -> Result<(), JobError> {
    // Reload config to get the secret key for decrypting the per-server API
    // key. This is cheap (env-var read) and avoids passing config into every
    // job handler signature.
    let config = load_config().map_err(|e| JobError::ExecutionFailed(e.to_string()))?;
    let svc = StreamingService::new(db.clone(), &config.auth.secret_key);
    let server = svc
        .get_server(&payload.server_id)
        .await
        .map_err(|e| JobError::ExecutionFailed(e.to_string()))?;
    let connector = svc
        .connector_for(&server)
        .map_err(|e| JobError::ExecutionFailed(e.to_string()))?;
    let items = connector
        .sync_library()
        .await
        .map_err(|e| JobError::ExecutionFailed(e.to_string()))?;
    let count = svc
        .sync_server_library(&payload.server_id, items)
        .await
        .map_err(|e| JobError::ExecutionFailed(e.to_string()))?;
    tracing::info!(server_id = %payload.server_id, count, "beam sync complete");
    Ok(())
}
