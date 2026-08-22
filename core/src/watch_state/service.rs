use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use serde::{Deserialize, Serialize};

use crate::entity::{episode_watch_status, watch_status};

use super::{WatchStateError, repo};
pub use repo::LibraryFilter;

const VALID_STATUSES: &[&str] = &["planning", "watching", "completed", "dropped", "paused"];

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

    /// One page of the user's tracked media, newest first.
    ///
    /// Returns the raw watch-state rows plus the total page count. Overlaying
    /// cameo metadata is the caller's job — the API layer already holds the
    /// `MediaService` needed for it, and keeping this crate's watch-state
    /// service free of a cameo dependency keeps it usable without a network.
    pub async fn list(
        &self,
        user_id: &str,
        filter: &LibraryFilter,
        page: u32,
        per_page: u64,
    ) -> Result<(Vec<watch_status::Model>, u32), WatchStateError> {
        repo::list_statuses(&self.db, user_id, filter, page, per_page).await
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
        if !VALID_STATUSES.contains(&status.as_str()) {
            return Err(WatchStateError::InvalidStatus(status));
        }
        if let Some(s) = score
            && !(0..=100).contains(&s)
        {
            return Err(WatchStateError::InvalidScore);
        }

        let now = Utc::now();
        let model = watch_status::ActiveModel {
            user_id: Set(user_id.to_string()),
            cameo_media_id: Set(cameo_media_id.to_string()),
            status: Set(status),
            score: Set(score),
            score_date: Set(score.map(|_| now.into())),
            started_at: Set(None),
            completed_at: Set(None),
            repeat_count: Set(0),
            notes: Set(notes),
            is_favorite: Set(is_favorite.unwrap_or(false)),
            updated_at: Set(now.into()),
        };

        repo::upsert_status(&self.db, model).await
    }

    pub async fn get_status(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<watch_status::Model, WatchStateError> {
        repo::find_status(&self.db, user_id, cameo_media_id)
            .await?
            .ok_or(WatchStateError::NotFound)
    }

    pub async fn delete_status(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<(), WatchStateError> {
        let deleted = repo::delete_status(&self.db, user_id, cameo_media_id).await?;
        if deleted {
            Ok(())
        } else {
            Err(WatchStateError::NotFound)
        }
    }

    pub async fn set_episode_status(
        &self,
        user_id: &str,
        cameo_episode_id: &str,
        cameo_media_id: &str,
        watched: bool,
        progress_seconds: Option<i32>,
    ) -> Result<episode_watch_status::Model, WatchStateError> {
        let now = Utc::now();
        let model = episode_watch_status::ActiveModel {
            user_id: Set(user_id.to_string()),
            cameo_episode_id: Set(cameo_episode_id.to_string()),
            cameo_media_id: Set(cameo_media_id.to_string()),
            season_number: Set(None),
            episode_number: Set(None),
            watched: Set(watched),
            watched_at: Set(if watched { Some(now.into()) } else { None }),
            progress_seconds: Set(progress_seconds),
            updated_at: Set(now.into()),
        };

        repo::upsert_episode_status(&self.db, model).await
    }

    pub async fn get_episode_status(
        &self,
        user_id: &str,
        cameo_episode_id: &str,
    ) -> Result<episode_watch_status::Model, WatchStateError> {
        repo::find_episode_status(&self.db, user_id, cameo_episode_id)
            .await?
            .ok_or(WatchStateError::NotFound)
    }

    pub async fn bulk_update(
        &self,
        user_id: &str,
        entries: Vec<BulkEntry>,
    ) -> Result<Vec<watch_status::Model>, WatchStateError> {
        let mut results = Vec::with_capacity(entries.len());
        for entry in entries {
            let model = self
                .set_status(
                    user_id,
                    &entry.cameo_media_id,
                    entry.status,
                    entry.score,
                    entry.notes,
                    entry.is_favorite,
                )
                .await?;
            results.push(model);
        }
        Ok(results)
    }
}
