pub mod apprise;
pub mod webhook;

use crate::notifications::NotificationError;
use crate::notifications::service::ResolvedNotification;

#[allow(async_fn_in_trait)]
pub trait NotificationChannel: Send + Sync {
    fn channel_type(&self) -> &'static str;

    async fn send(
        &self,
        notification: &ResolvedNotification,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError>;
}
