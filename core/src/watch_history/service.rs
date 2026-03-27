use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use ulid::Ulid;

use crate::entity::watch_history;
use crate::pagination::{CursorParams, PaginatedResult};

use super::{WatchHistoryError, repo};

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
        let model = watch_history::ActiveModel {
            id: Set(Ulid::new().to_string()),
            user_id: Set(params.user_id.to_string()),
            cameo_media_id: Set(params.cameo_media_id.to_string()),
            cameo_episode_id: Set(params.cameo_episode_id.map(String::from)),
            event: Set(params.event),
            position_seconds: Set(params.position_seconds),
            metadata: Set(params.metadata),
            source: Set(params.source),
            timestamp: Set(Utc::now().into()),
        };

        repo::insert_event(&self.db, model).await
    }

    pub async fn list_history(
        &self,
        user_id: &str,
        cursor: CursorParams,
        cameo_media_id: Option<&str>,
    ) -> Result<PaginatedResult<watch_history::Model>, WatchHistoryError> {
        repo::list_for_user(&self.db, user_id, cursor, cameo_media_id).await
    }

    pub async fn backfill(
        &self,
        _user_id: &str,
        entries: Vec<watch_history::ActiveModel>,
    ) -> Result<(), WatchHistoryError> {
        repo::bulk_insert(&self.db, entries).await.map(|_| ())
    }
}
