pub mod apprise;
pub mod webhook;

pub use apprise::{AppriseChannel, AppriseChannelConfig};
pub use webhook::{WebhookChannel, WebhookChannelConfig};

use serde::{Deserialize, Serialize};

use crate::notifications::NotificationError;

/// A notification rendered to a human-readable form, ready to deliver. The
/// [`NotificationService`](super::NotificationService) resolves the source
/// event into this shape (looking up titles via cameo) so individual channels
/// only have to think about transport, not content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedNotification {
    pub event_type: String,
    pub title: String,
    pub body: String,
}

#[allow(async_fn_in_trait)]
pub trait NotificationChannel: Send + Sync {
    fn channel_type(&self) -> &'static str;

    async fn send(
        &self,
        message: &RenderedNotification,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError>;
}

/// A delivery URL must be absolute `http` or `https` with a host.
pub(crate) fn validate_url(raw: &str) -> Result<(), NotificationError> {
    let url = reqwest::Url::parse(raw)
        .map_err(|e| NotificationError::InvalidConfig(format!("url: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(NotificationError::InvalidConfig(
            "url must be an absolute http or https URL".into(),
        ));
    }
    Ok(())
}
