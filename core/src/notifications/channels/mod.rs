pub mod apprise;
pub mod webhook;

use crate::notifications::NotificationError;
use crate::notifications::service::NotificationEvent;

pub trait NotificationChannel: Send + Sync {
    fn channel_type(&self) -> &'static str;

    async fn send(
        &self,
        event: &NotificationEvent,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError>;
}
