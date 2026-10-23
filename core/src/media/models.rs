use serde::{Deserialize, Serialize};

use crate::entity::enums::EntryStatus;
use crate::entity::media;

use super::id::{MediaId, MediaKind};

/// A media item enriched with the current user's library state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaWithUserState {
    /// The public, self-describing id (`tmdb:movie:550`).
    pub cameo_media_id: String,
    pub title: String,
    pub kind: MediaKind,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    // Enriched user data
    pub watch_status: Option<EntryStatus>,
    /// Whether the user is on a rewatch (their current run is not the first).
    pub rewatching: bool,
    pub score: Option<i32>,
    pub is_favorite: bool,
}

impl MediaWithUserState {
    /// A base item (no user state) built from a catalog snapshot row.
    ///
    /// This is how the library renders without calling a provider: everything
    /// shown comes from the stored snapshot.
    pub fn from_snapshot(m: &media::Model, public_id: &MediaId) -> Self {
        Self {
            cameo_media_id: public_id.to_string(),
            title: m.title.clone(),
            kind: m.kind,
            year: m.start_date.map(|d| chrono::Datelike::year(&d)),
            overview: m.overview.clone(),
            poster_url: m.poster_url.clone(),
            backdrop_url: m.backdrop_url.clone(),
            genres: m.genres.clone(),
            rating: m.raw.as_ref().and_then(raw_vote_average),
            watch_status: None,
            rewatching: false,
            score: None,
            is_favorite: false,
        }
    }
}

/// The provider rating kept inside a snapshot's `raw` payload, if any.
fn raw_vote_average(raw: &serde_json::Value) -> Option<f64> {
    ["movie", "show"]
        .iter()
        .find_map(|k| raw.get(k)?.get("vote_average")?.as_f64())
}

/// Full detail for a single media item — a [`MediaWithUserState`] plus the
/// extended metadata shown on a detail page.
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaDetails {
    /// The base media item together with the current user's library state.
    pub media: MediaWithUserState,
    /// Marketing tagline.
    pub tagline: Option<String>,
    /// Runtime in minutes — the film runtime for a movie, or the typical
    /// episode runtime for a show.
    pub runtime_minutes: Option<u32>,
    /// Release status as the provider words it (e.g. "Returning Series").
    pub status: Option<String>,
    /// Official homepage URL.
    pub homepage: Option<String>,
    /// IMDB id (movies only).
    pub imdb_id: Option<String>,
    /// Number of seasons (shows only).
    pub number_of_seasons: Option<u32>,
    /// Number of episodes (shows only).
    pub number_of_episodes: Option<u32>,
    /// Whether the show is still in production (shows only).
    pub in_production: Option<bool>,
}

/// Summary of one season of a show.
#[derive(Debug, Serialize, Deserialize)]
pub struct SeasonInfo {
    pub season_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub poster_url: Option<String>,
    pub episode_count: u32,
}

/// Summary of one episode of a show.
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
