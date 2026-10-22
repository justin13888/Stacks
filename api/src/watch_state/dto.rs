use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WatchStatusRequest {
    pub status: String,
    pub score: Option<i32>,
    pub notes: Option<String>,
    pub is_favorite: Option<bool>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WatchStatusResponse {
    pub cameo_media_id: String,
    pub status: String,
    pub score: Option<i32>,
    pub notes: Option<String>,
    pub is_favorite: bool,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

/// A page of the user's tracked media.
///
/// Items reuse [`crate::media::dto::MediaItem`] so the Library page renders with
/// the same component as search and discover.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LibraryResponse {
    pub items: Vec<crate::media::dto::MediaItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EpisodeStatusRequest {
    pub watched: bool,
    pub progress_seconds: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EpisodeStatusResponse {
    pub cameo_episode_id: String,
    pub watched: bool,
    pub progress_seconds: Option<i32>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkEntry {
    pub cameo_media_id: String,
    pub status: String,
    pub score: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkStatusRequest {
    pub entries: Vec<BulkEntry>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BulkStatusResponse {
    pub updated: u32,
}
