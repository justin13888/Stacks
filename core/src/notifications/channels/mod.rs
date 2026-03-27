pub mod apprise;
pub mod webhook;

use crate::notifications::NotificationError;
use crate::notifications::service::NotificationEvent;

#[allow(async_fn_in_trait)]
pub trait NotificationChannel: Send + Sync {
    fn channel_type(&self) -> &'static str;

    async fn send(
        &self,
        event: &NotificationEvent,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError>;
}
