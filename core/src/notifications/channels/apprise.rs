use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::notifications::error::NotificationError;
use crate::notifications::service::NotificationEvent;

use super::NotificationChannel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppriseChannelConfig {
    pub url: String,
    pub tags: Option<Vec<String>>,
    pub title_template: Option<String>,
    pub body_template: Option<String>,
}

pub struct AppriseChannel {
    http: Client,
}

impl AppriseChannel {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

impl Default for AppriseChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationChannel for AppriseChannel {
    fn channel_type(&self) -> &'static str {
        "apprise"
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
