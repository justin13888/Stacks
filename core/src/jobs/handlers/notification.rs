use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::jobs::{JobContext, JobError};
use crate::notifications::NotificationService;
use crate::notifications::service::NotificationEvent;

/// Payload for a `notification` job.
#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationPayload {
    pub user_id: Uuid,
    pub event: NotificationEvent,
}

/// Resolve the user's subscriptions for the event type, then dispatch.
pub async fn handle(ctx: &JobContext, payload: NotificationPayload) -> Result<(), JobError> {
    let svc = NotificationService::new(ctx.db.clone());
    svc.dispatch(payload.user_id, payload.event)
        .await
        .map_err(|e| JobError::ExecutionFailed(format!("notification dispatch: {e}")))?;
    Ok(())
}
