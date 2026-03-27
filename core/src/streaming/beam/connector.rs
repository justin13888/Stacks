use std::collections::HashMap;

use super::client::BeamClient;
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

impl StreamingConnector for BeamConnector {
    fn server_type(&self) -> &'static str {
        "beam"
    }

    async fn sync_library(&self) -> Result<Vec<ConnectorLibraryItem>, StreamingError> {
        todo!()
    }

    async fn playback_link(
        &self,
        server_item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        let _ = (server_item_id, episode_id);
        todo!()
    }

    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &HashMap<String, String>,
    ) -> Result<Vec<ConnectorEvent>, StreamingError> {
        let _ = (payload, headers);
        todo!()
    }

    async fn health_check(&self) -> Result<ConnectorHealth, StreamingError> {
        todo!()
    }
}
