use serde::{Deserialize, Serialize};

use crate::jobs::{JobContext, JobError};
use crate::streaming::StreamingService;

/// Payload for a `beam_sync` job.
#[derive(Debug, Serialize, Deserialize)]
pub struct BeamSyncPayload {
    pub server_id: String,
}

/// Fetch a Beam server's library and persist updated `server_library_item`
/// records. Errors propagate up so the runner can decide whether to retry.
pub async fn handle(ctx: &JobContext, payload: BeamSyncPayload) -> Result<(), JobError> {
    let svc = StreamingService::new(ctx.db.clone(), ctx.encryption_key);
    let synced = svc
        .sync_server(&payload.server_id)
        .await
        .map_err(|e| JobError::ExecutionFailed(format!("beam_sync: {e}")))?;
    tracing::info!(server_id = %payload.server_id, synced, "beam_sync job complete");
    Ok(())
}
