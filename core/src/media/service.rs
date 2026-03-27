use sea_orm::DatabaseConnection;

use crate::pagination::{CursorParams, PaginatedResult};

use super::{MediaError, filter::FilterQuery, models::MediaWithUserState};

pub struct MediaService {
    db: DatabaseConnection,
}

impl MediaService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn search(
        &self,
        _user_id: &str,
        _query: &str,
        _filter: &FilterQuery,
        _pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        todo!("MediaService::search — delegate to cameo then enrich")
    }

    pub async fn get_detail(
        &self,
        _user_id: &str,
        _cameo_id: &str,
    ) -> Result<MediaWithUserState, MediaError> {
        todo!("MediaService::get_detail")
    }

    pub async fn get_seasons(&self, _cameo_id: &str) -> Result<Vec<serde_json::Value>, MediaError> {
        todo!("MediaService::get_seasons — delegate to cameo")
    }

    pub async fn get_episodes(
        &self,
        _cameo_id: &str,
        _season: u32,
    ) -> Result<Vec<serde_json::Value>, MediaError> {
        todo!("MediaService::get_episodes — delegate to cameo")
    }

    pub async fn get_recommendations(
        &self,
        _user_id: &str,
        _cameo_id: &str,
        _pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        todo!("MediaService::get_recommendations")
    }

    pub async fn discover(
        &self,
        _user_id: &str,
        _category: &str,
        _media_type: Option<&str>,
        _pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        todo!("MediaService::discover — trending/popular/upcoming/top_rated")
    }
}
