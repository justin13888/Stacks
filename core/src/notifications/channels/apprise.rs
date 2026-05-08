use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::notifications::error::NotificationError;
use crate::notifications::service::ResolvedNotification;

use super::NotificationChannel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppriseChannelConfig {
    /// Base URL of the user's Apprise API server (e.g.
    /// `https://apprise.example.com`).
    pub url: String,
    /// Optional Apprise tag(s) to target a subset of configured services.
    pub tags: Option<Vec<String>>,
    /// Optional override for the notification title.
    pub title_template: Option<String>,
    /// Optional override for the notification body.
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
        notification: &ResolvedNotification,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError> {
        let cfg: AppriseChannelConfig = serde_json::from_value(config.clone())
            .map_err(|e| NotificationError::InvalidConfig(e.to_string()))?;

        let title = cfg
            .title_template
            .clone()
            .unwrap_or_else(|| notification.title.clone());
        let body = cfg
            .body_template
            .clone()
            .unwrap_or_else(|| notification.body.clone());

        let mut payload = serde_json::json!({
            "title": title,
            "body": body,
            "format": "text",
        });
        if let Some(tags) = cfg.tags.as_ref() {
            payload["tag"] = serde_json::Value::String(tags.join(","));
        }

        let url = format!(
            "{}/notify",
            cfg.url.trim_end_matches('/')
        );
        let res = self
            .http
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotificationError::DeliveryFailed(e.to_string()))?;
        if !res.status().is_success() {
            return Err(NotificationError::DeliveryFailed(format!(
                "apprise returned {}",
                res.status()
            )));
        }
        Ok(())
    }
}
