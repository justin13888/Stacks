use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::notifications::error::NotificationError;
use crate::notifications::service::ResolvedNotification;

use super::NotificationChannel;

type HmacSha256 = Hmac<Sha256>;

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
        notification: &ResolvedNotification,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError> {
        let cfg: WebhookChannelConfig = serde_json::from_value(config.clone())
            .map_err(|e| NotificationError::InvalidConfig(e.to_string()))?;

        let body = serde_json::json!({
            "title": notification.title,
            "body": notification.body,
            "event_type": notification.event.event_type(),
            "event": notification.event,
        });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| NotificationError::DeliveryFailed(e.to_string()))?;

        let method = cfg.method.as_deref().unwrap_or("POST").to_uppercase();
        let mut req = match method.as_str() {
            "PUT" => self.http.put(&cfg.url),
            _ => self.http.post(&cfg.url),
        }
        .header("Content-Type", "application/json")
        .body(body_bytes.clone());

        if let Some(secret) = cfg.secret.as_deref() {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                .map_err(|e| NotificationError::DeliveryFailed(e.to_string()))?;
            mac.update(&body_bytes);
            let signature = hex::encode(mac.finalize().into_bytes());
            req = req.header("X-Stacks-Signature", signature);
        }

        let res = req
            .send()
            .await
            .map_err(|e| NotificationError::DeliveryFailed(e.to_string()))?;
        if !res.status().is_success() {
            return Err(NotificationError::DeliveryFailed(format!(
                "webhook returned {}",
                res.status()
            )));
        }
        Ok(())
    }
}
