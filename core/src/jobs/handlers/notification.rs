use serde::{Deserialize, Serialize};

use crate::jobs::{JobContext, JobError};
use crate::notifications::NotificationService;
use crate::notifications::service::NotificationEvent;

/// Payload for a `notification` job.
#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub user_id: String,
    pub event: NotificationEvent,
}

/// Resolve enabled channels for the user + event type, then dispatch.
pub async fn handle(ctx: &JobContext, payload: NotificationPayload) -> Result<(), JobError> {
    let svc = NotificationService::new(ctx.db.clone());
    svc.dispatch(&ctx.cameo, &payload.user_id, payload.event)
        .await
        .map_err(|e| JobError::ExecutionFailed(format!("notification dispatch: {e}")))?;
    Ok(())
}
