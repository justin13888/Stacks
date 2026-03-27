use sea_orm::DatabaseConnection;

use crate::entity::{server_library_item, streaming_server};

use super::StreamingError;
use super::models::{ConnectorEvent, ConnectorHealth, ConnectorLibraryItem};

pub struct StreamingService {
    pub db: DatabaseConnection,
}

impl StreamingService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn register_server(
        &self,
        user_id: Option<&str>,
        name: String,
        server_type: String,
        base_url: String,
        api_key: Option<String>,
        webhook_secret: Option<String>,
    ) -> Result<streaming_server::Model, StreamingError> {
        let _ = (
            user_id,
            name,
            server_type,
            base_url,
            api_key,
            webhook_secret,
        );
        todo!()
    }

    pub async fn get_server(&self, id: &str) -> Result<streaming_server::Model, StreamingError> {
        let _ = id;
        todo!()
    }

    pub async fn list_servers(
        &self,
        user_id: &str,
    ) -> Result<Vec<streaming_server::Model>, StreamingError> {
        let _ = user_id;
        todo!()
    }

    pub async fn delete_server(&self, id: &str) -> Result<(), StreamingError> {
        let _ = id;
        todo!()
    }

    pub async fn sync_server_library(
        &self,
        server_id: &str,
        items: Vec<ConnectorLibraryItem>,
    ) -> Result<u64, StreamingError> {
        let _ = (server_id, items);
        todo!()
    }

    pub async fn get_playback_link(
        &self,
        server_id: &str,
        cameo_media_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        let _ = (server_id, cameo_media_id, episode_id);
        todo!()
    }

    pub async fn handle_webhook_events(
        &self,
        server_id: &str,
        events: Vec<ConnectorEvent>,
    ) -> Result<(), StreamingError> {
        let _ = (server_id, events);
        todo!()
    }

    pub async fn health_check(&self, server_id: &str) -> Result<ConnectorHealth, StreamingError> {
        let _ = server_id;
        todo!()
    }

    pub async fn get_library_items(
        &self,
        server_id: &str,
    ) -> Result<Vec<server_library_item::Model>, StreamingError> {
        let _ = server_id;
        todo!()
    }
}
