use sea_orm::DatabaseConnection;

use crate::entity::watch_history;
use crate::pagination::{CursorParams, PaginatedResult};

use super::WatchHistoryError;

pub struct WatchHistoryService {
    pub db: DatabaseConnection,
}

impl WatchHistoryService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn record_event(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        cameo_episode_id: Option<&str>,
        event: String,
        position_seconds: Option<i32>,
        metadata: Option<serde_json::Value>,
        source: String,
    ) -> Result<watch_history::Model, WatchHistoryError> {
        let _ = (
            user_id,
            cameo_media_id,
            cameo_episode_id,
            event,
            position_seconds,
            metadata,
            source,
        );
        todo!()
    }

    pub async fn list_history(
        &self,
        user_id: &str,
        cursor: CursorParams,
        cameo_media_id: Option<&str>,
    ) -> Result<PaginatedResult<watch_history::Model>, WatchHistoryError> {
        let _ = (user_id, cursor, cameo_media_id);
        todo!()
    }

    pub async fn backfill(
        &self,
        user_id: &str,
        entries: Vec<watch_history::ActiveModel>,
    ) -> Result<(), WatchHistoryError> {
        let _ = (user_id, entries);
        todo!()
    }
}
