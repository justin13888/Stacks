use serde::{Deserialize, Serialize};

use super::{DeliveryClient, NotificationChannel, RenderedNotification};
use crate::notifications::error::NotificationError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppriseChannelConfig {
    /// Full URL of an Apprise notify endpoint, e.g.
    /// `https://apprise.example.com/notify/{token}`.
    pub url: String,
    /// Optional Apprise tag filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl AppriseChannelConfig {
    /// Whether the configuration can be delivered: an absolute http(s) URL
    /// and tags that are not blank.
    pub fn validate(&self) -> Result<(), NotificationError> {
        super::validate_url(&self.url)?;
        if self.tags.iter().flatten().any(|t| t.trim().is_empty()) {
            return Err(NotificationError::InvalidConfig(
                "tags must not be blank".into(),
            ));
        }
        Ok(())
    }
}

/// JSON body POSTed to an Apprise endpoint.
#[derive(Debug, Serialize)]
struct AppriseBody<'a> {
    title: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(rename = "type")]
    notification_type: &'static str,
}

pub struct AppriseChannel {
    client: DeliveryClient,
}

impl AppriseChannel {
    /// A channel on the shared [`DeliveryClient`].
    pub fn new() -> Self {
        Self::with_client(DeliveryClient::shared())
    }

    pub fn with_client(client: DeliveryClient) -> Self {
        Self { client }
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
        message: &RenderedNotification,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError> {
        let cfg: AppriseChannelConfig = serde_json::from_value(config.clone())
            .map_err(|e| NotificationError::InvalidConfig(e.to_string()))?;
        let body = AppriseBody {
            title: &message.title,
            body: &message.body,
            tag: cfg.tags.as_ref().map(|t| t.join(",")),
            notification_type: "info",
        };
        let resp = self
            .client
            .http()
            .post(&cfg.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| self.client.send_error(e))?;
        if !resp.status().is_success() {
            return Err(NotificationError::DeliveryFailed(format!(
                "apprise responded {}",
                resp.status()
            )));
        }
        Ok(())
    }
}
