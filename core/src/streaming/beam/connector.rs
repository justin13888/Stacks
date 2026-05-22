use std::collections::HashMap;

use super::client::{BeamClient, BeamLibraryItem};
use super::webhook;
use crate::streaming::StreamingError;
use crate::streaming::connector::StreamingConnector;
use crate::streaming::models::{ConnectorEvent, ConnectorHealth, ConnectorLibraryItem};

pub struct BeamConnector {
    pub client: BeamClient,
    pub webhook_secret: Option<String>,
}

impl BeamConnector {
    pub fn new(base_url: String, api_key: String, webhook_secret: Option<String>) -> Self {
        Self {
            client: BeamClient::new(base_url, api_key),
            webhook_secret,
        }
    }
}

/// Map a Beam library item to a connector-level item.
///
/// Beam's per-item `media_type` is folded into `external_ids` (under the
/// `media_type` key) so [`matcher`](crate::streaming::matcher) can tell movies
/// from TV shows without a dedicated field.
fn to_connector_item(client: &BeamClient, item: BeamLibraryItem) -> ConnectorLibraryItem {
    let mut external_ids = match item.provider_ids {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    external_ids.insert(
        "media_type".to_string(),
        serde_json::Value::String(item.media_type),
    );
    if let Some(year) = item.year {
        external_ids
            .entry("year")
            .or_insert(serde_json::Value::from(year));
    }
    ConnectorLibraryItem {
        server_internal_id: item.id.clone(),
        title: item.title,
        quality: None,
        deep_link: format!("{}/library/{}", client.base_url, item.id),
        playback_link: format!("{}/api/library/{}/playback", client.base_url, item.id),
        external_ids: Some(serde_json::Value::Object(external_ids)),
    }
}

impl StreamingConnector for BeamConnector {
    fn server_type(&self) -> &'static str {
        "beam"
    }

    async fn sync_library(&self) -> Result<Vec<ConnectorLibraryItem>, StreamingError> {
        let items = self.client.get_library().await?;
        Ok(items
            .into_iter()
            .map(|item| to_connector_item(&self.client, item))
            .collect())
    }

    async fn playback_link(
        &self,
        server_item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        self.client
            .get_playback_url(server_item_id, episode_id)
            .await
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &HashMap<String, String>,
    ) -> Result<Vec<ConnectorEvent>, StreamingError> {
        // When a secret is configured the signature must verify; when it is
        // absent the payload is accepted as-is (unauthenticated server).
        if let Some(secret) = &self.webhook_secret {
            let signature = headers
                .get("x-beam-signature")
                .or_else(|| headers.get("x-hub-signature-256"))
                .map(String::as_str);
            webhook::verify_signature(payload, signature, secret)?;
        }
        let parsed = webhook::parse_payload(payload)?;
        webhook::to_connector_events(parsed)
    }

    async fn health_check(&self) -> Result<ConnectorHealth, StreamingError> {
        // A failed reachability check is a *reported* unhealthy state, not an
        // error — the caller always gets a `ConnectorHealth`.
        match self.client.ping().await {
            Ok(version) => Ok(ConnectorHealth {
                healthy: true,
                version: Some(version),
                message: None,
            }),
            Err(e) => Ok(ConnectorHealth {
                healthy: false,
                version: None,
                message: Some(e.to_string()),
            }),
        }
    }
}
