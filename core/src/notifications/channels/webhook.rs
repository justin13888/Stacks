use hmac::{Hmac, KeyInit, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::{NotificationChannel, RenderedNotification};
use crate::notifications::error::NotificationError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookChannelConfig {
    pub url: String,
    /// Optional HMAC-SHA256 secret. When set, the request body is signed and
    /// the hex digest is sent in the `X-Stacks-Signature` header so the
    /// receiver can verify the call came from Stacks.
    pub secret: Option<String>,
    /// HTTP method (default `POST`).
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
        message: &RenderedNotification,
        config: &serde_json::Value,
    ) -> Result<(), NotificationError> {
        let cfg: WebhookChannelConfig = serde_json::from_value(config.clone())
            .map_err(|e| NotificationError::InvalidConfig(e.to_string()))?;
        let body = serde_json::to_vec(message)
            .map_err(|e| NotificationError::DeliveryFailed(e.to_string()))?;

        let mut req = self
            .http
            .request(parse_method(cfg.method.as_deref()), &cfg.url)
            .header("Content-Type", "application/json");
        if let Some(secret) = &cfg.secret {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                .map_err(|_| NotificationError::InvalidConfig("invalid HMAC secret".into()))?;
            mac.update(&body);
            req = req.header(
                "X-Stacks-Signature",
                hex::encode(mac.finalize().into_bytes()),
            );
        }

        let resp = req
            .body(body)
            .send()
            .await
            .map_err(|e| NotificationError::DeliveryFailed(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(NotificationError::DeliveryFailed(format!(
                "webhook responded {status}"
            )));
        }
        Ok(())
    }
}

fn parse_method(method: Option<&str>) -> reqwest::Method {
    method
        .and_then(|m| m.parse().ok())
        .unwrap_or(reqwest::Method::POST)
}
