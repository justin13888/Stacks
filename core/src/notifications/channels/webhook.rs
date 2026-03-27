use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::notifications::error::NotificationError;
use crate::notifications::service::NotificationEvent;

use super::NotificationChannel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookChannelConfig {
    pub url: String,
    pub secret: Option<String>,
    pub method: Option<String>,
}

pub struct WebhookChannel {
    http: Client,
}

impl WebhookChannel {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

impl Default for WebhookChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationChannel for WebhookChannel {
    fn channel_type(&self) -> &'static str {
        "webhook"
    }

    async fn send(
        &self,
        event: &NotificationEvent,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError> {
        let _ = (event, config);
        todo!()
    }
}
