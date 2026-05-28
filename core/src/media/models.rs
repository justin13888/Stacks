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

/// Full detail for a single media item — a [`MediaWithUserState`] plus the
/// extended metadata shown on a detail page.
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaDetails {
    /// The base media item together with the current user's watch state.
    pub media: MediaWithUserState,
    /// Marketing tagline.
    pub tagline: Option<String>,
    /// Runtime in minutes — the film runtime for a movie, or the typical
    /// episode runtime for a TV show.
    pub runtime_minutes: Option<u32>,
    /// Release status (e.g. "Released", "Returning Series").
    pub status: Option<String>,
    /// Official homepage URL.
    pub homepage: Option<String>,
    /// IMDB id (movies only).
    pub imdb_id: Option<String>,
    /// Number of seasons (TV shows only).
    pub number_of_seasons: Option<u32>,
    /// Number of episodes (TV shows only).
    pub number_of_episodes: Option<u32>,
    /// Whether the show is still in production (TV shows only).
    pub in_production: Option<bool>,
}

/// Summary of one season of a TV show.
#[derive(Debug, Serialize, Deserialize)]
pub struct SeasonInfo {
    pub season_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub poster_url: Option<String>,
    pub episode_count: u32,
}

/// Summary of one episode of a TV show.
#[derive(Debug, Serialize, Deserialize)]
pub struct EpisodeInfo {
    pub season_number: u32,
    pub episode_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub still_url: Option<String>,
    pub rating: Option<f64>,
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
