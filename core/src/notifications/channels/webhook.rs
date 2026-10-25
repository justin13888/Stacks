use std::collections::BTreeMap;

use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::{HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::{DeliveryClient, NotificationChannel, RenderedNotification};
use crate::notifications::error::NotificationError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookChannelConfig {
    pub url: String,
    /// Optional HMAC-SHA256 secret. When set, the request body is signed and
    /// the hex digest is sent in the `X-Stacks-Signature` header so the
    /// receiver can verify the call came from Stacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// HTTP method: one of [`METHODS`] (default `POST`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Extra request headers (an `Authorization` the receiver expects, for
    /// instance), sent on every delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
}

/// The methods a webhook may be delivered with.
pub const METHODS: &[&str] = &["POST", "PUT", "PATCH"];

/// Headers a webhook may not set: the ones Stacks writes itself, and the ones
/// that frame the request.
const RESERVED_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "host",
    "transfer-encoding",
    "x-stacks-signature",
];

impl WebhookChannelConfig {
    /// Whether the configuration can be delivered: an absolute http(s) URL,
    /// a known method, a non-empty secret, and headers that are valid and
    /// not ones Stacks writes itself.
    pub fn validate(&self) -> Result<(), NotificationError> {
        super::validate_url(&self.url)?;
        if let Some(method) = &self.method
            && !METHODS.contains(&method.as_str())
        {
            return Err(NotificationError::InvalidConfig(format!(
                "method must be one of {}",
                METHODS.join(", ")
            )));
        }
        if self.secret.as_deref().is_some_and(str::is_empty) {
            return Err(NotificationError::InvalidConfig(
                "secret must not be empty".into(),
            ));
        }
        for (name, value) in self.headers.iter().flatten() {
            let parsed = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                NotificationError::InvalidConfig(format!("`{name}` is not a valid header name"))
            })?;
            if RESERVED_HEADERS.contains(&parsed.as_str()) {
                return Err(NotificationError::InvalidConfig(format!(
                    "the `{name}` header is set by Stacks"
                )));
            }
            HeaderValue::from_str(value).map_err(|_| {
                NotificationError::InvalidConfig(format!("the `{name}` header's value is invalid"))
            })?;
        }
        Ok(())
    }
}

pub struct WebhookChannel {
    client: DeliveryClient,
}

impl WebhookChannel {
    /// A channel on the shared [`DeliveryClient`].
    pub fn new() -> Self {
        Self::with_client(DeliveryClient::shared())
    }

    pub fn with_client(client: DeliveryClient) -> Self {
        Self { client }
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
            .client
            .http()
            .request(parse_method(cfg.method.as_deref()), &cfg.url)
            .header("Content-Type", "application/json");
        for (name, value) in cfg.headers.iter().flatten() {
            req = req.header(name.as_str(), value.as_str());
        }
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
            .map_err(|e| self.client.send_error(e))?;
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
