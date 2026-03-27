use std::collections::HashMap;

use super::error::StreamingError;
use super::models::{ConnectorEvent, ConnectorHealth, ConnectorLibraryItem};

pub trait StreamingConnector: Send + Sync {
    fn server_type(&self) -> &'static str;

    async fn sync_library(&self) -> Result<Vec<ConnectorLibraryItem>, StreamingError>;

    async fn playback_link(
        &self,
        server_item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError>;

    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &HashMap<String, String>,
    ) -> Result<Vec<ConnectorEvent>, StreamingError>;

    async fn health_check(&self) -> Result<ConnectorHealth, StreamingError>;
}
