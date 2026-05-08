use std::collections::HashMap;

use async_trait::async_trait;

use super::client::BeamClient;
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

#[async_trait]
impl StreamingConnector for BeamConnector {
    fn server_type(&self) -> &'static str {
        "beam"
    }

    async fn sync_library(&self) -> Result<Vec<ConnectorLibraryItem>, StreamingError> {
        let items = self.client.get_library().await?;
        Ok(items
            .into_iter()
            .map(|i| ConnectorLibraryItem {
                server_internal_id: i.id.clone(),
                title: i.title,
                quality: None,
                deep_link: format!(
                    "{}/items/{}",
                    self.client.base_url.trim_end_matches('/'),
                    i.id
                ),
                playback_link: format!(
                    "{}/api/v1/items/{}/playback",
                    self.client.base_url.trim_end_matches('/'),
                    i.id
                ),
                external_ids: i.provider_ids,
            })
            .collect())
    }

    async fn playback_link(
        &self,
        server_item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        self.client.get_playback_url(server_item_id, episode_id).await
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &HashMap<String, String>,
    ) -> Result<Vec<ConnectorEvent>, StreamingError> {
        if let Some(secret) = &self.webhook_secret {
            let signature = headers
                .get("x-beam-signature")
                .or_else(|| headers.get("X-Beam-Signature"))
                .map(|s| s.as_str());
            webhook::verify_signature(payload, signature, secret)?;
        }
        let parsed = webhook::parse_payload(payload)?;
        webhook::to_connector_events(parsed)
    }

    async fn health_check(&self) -> Result<ConnectorHealth, StreamingError> {
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
