use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::entity::{episode_watch_status, watch_status};

use super::WatchStateError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkEntry {
    pub cameo_media_id: String,
    pub status: String,
    pub score: Option<i32>,
    pub notes: Option<String>,
    pub is_favorite: Option<bool>,
}

pub struct WatchStateService {
    pub db: DatabaseConnection,
}

impl WatchStateService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn set_status(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        status: String,
        score: Option<i32>,
        notes: Option<String>,
        is_favorite: Option<bool>,
    ) -> Result<watch_status::Model, WatchStateError> {
        let _ = (user_id, cameo_media_id, status, score, notes, is_favorite);
        todo!()
    }

    pub async fn get_status(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<watch_status::Model, WatchStateError> {
        let _ = (user_id, cameo_media_id);
        todo!()
    }

    pub async fn delete_status(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<(), WatchStateError> {
        let _ = (user_id, cameo_media_id);
        todo!()
    }

    pub async fn set_episode_status(
        &self,
        user_id: &str,
        cameo_episode_id: &str,
        cameo_media_id: &str,
        watched: bool,
        progress_seconds: Option<i32>,
    ) -> Result<episode_watch_status::Model, WatchStateError> {
        let _ = (
            user_id,
            cameo_episode_id,
            cameo_media_id,
            watched,
            progress_seconds,
        );
        todo!()
    }

    pub async fn get_episode_status(
        &self,
        user_id: &str,
        cameo_episode_id: &str,
    ) -> Result<episode_watch_status::Model, WatchStateError> {
        let _ = (user_id, cameo_episode_id);
        todo!()
    }

    pub async fn bulk_update(
        &self,
        user_id: &str,
        entries: Vec<BulkEntry>,
    ) -> Result<Vec<watch_status::Model>, WatchStateError> {
        let _ = (user_id, entries);
        todo!()
    }
}
