use serde::{Deserialize, Serialize};

/// A cameo media item enriched with user watch state and streaming availability.
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaWithUserState {
    pub cameo_media_id: String,
    pub title: String,
    pub media_type: String,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    // Enriched user data
    pub watch_status: Option<String>,
    pub score: Option<i32>,
    pub is_favorite: bool,
    pub availability: AvailabilityInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AvailabilityInfo {
    pub available: bool,
    pub servers: Vec<ServerAvailability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAvailability {
    pub server_id: String,
    pub server_name: String,
    pub quality: Option<String>,
    pub deep_link: String,
    pub playback_link: String,
}

/// External IDs used for matching between cameo and streaming servers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExternalIds {
    pub tmdb_id: Option<i64>,
    pub anilist_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub mal_id: Option<i64>,
}
