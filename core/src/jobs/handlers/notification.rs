use sea_orm::DatabaseConnection;

use crate::jobs::JobError;
use crate::notifications::service::NotificationEvent;

/// Payload for a Notification job.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NotificationPayload {
    pub user_id: String,
    pub event: NotificationEvent,
}

/// Handle a Notification job: resolve enabled channels for the user/event and
/// dispatch the notification through each channel.
pub async fn handle(db: &DatabaseConnection, payload: NotificationPayload) -> Result<(), JobError> {
    let _ = (db, payload);
    todo!()
}
