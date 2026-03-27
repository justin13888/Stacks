use sea_orm::DatabaseConnection;

use crate::entity::watch_history;
use crate::pagination::{CursorParams, PaginatedResult};

use super::WatchHistoryError;

pub struct RecordEventParams<'a> {
    pub user_id: &'a str,
    pub cameo_media_id: &'a str,
    pub cameo_episode_id: Option<&'a str>,
    pub event: String,
    pub position_seconds: Option<i32>,
    pub metadata: Option<serde_json::Value>,
    pub source: String,
}

pub struct WatchHistoryService {
    pub db: DatabaseConnection,
}

impl WatchHistoryService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn record_event(
        &self,
        params: RecordEventParams<'_>,
    ) -> Result<watch_history::Model, WatchHistoryError> {
        let _ = params;
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
