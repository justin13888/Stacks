use salvo::oapi::{ToParameters, ToSchema};
use serde::{Deserialize, Serialize};
use stacks_core::media::models::{
    AvailabilityInfo, EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo, ServerAvailability,
};

/// Query parameters accepted by the media search endpoint.
#[derive(Debug, Serialize, Deserialize, ToSchema, ToParameters)]
pub struct MediaSearchParams {
    /// Free-text search query.
    pub q: String,
    /// Page size hint (advisory for cameo-backed endpoints).
    pub limit: Option<u32>,
    /// Opaque pagination cursor.
    pub cursor: Option<String>,
}

/// One streaming server that carries a media item.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AvailabilityServer {
    pub server_id: String,
    pub server_name: String,
    pub quality: Option<String>,
    pub deep_link: String,
    pub playback_link: String,
}

impl From<ServerAvailability> for AvailabilityServer {
    fn from(s: ServerAvailability) -> Self {
        Self {
            server_id: s.server_id,
            server_name: s.server_name,
            quality: s.quality,
            deep_link: s.deep_link,
            playback_link: s.playback_link,
        }
    }
}

/// Streaming availability of a media item across the user's servers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Availability {
    pub available: bool,
    pub servers: Vec<AvailabilityServer>,
}

impl From<AvailabilityInfo> for Availability {
    fn from(a: AvailabilityInfo) -> Self {
        Self {
            available: a.available,
            servers: a
                .servers
                .into_iter()
                .map(AvailabilityServer::from)
                .collect(),
        }
    }
}

/// A media item (movie or TV show) carrying the current user's watch state.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MediaItem {
    pub cameo_media_id: String,
    pub title: String,
    pub media_type: String,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub watch_status: Option<String>,
    pub score: Option<i32>,
    pub is_favorite: bool,
    pub availability: Availability,
}

impl From<MediaWithUserState> for MediaItem {
    fn from(m: MediaWithUserState) -> Self {
        Self {
            cameo_media_id: m.cameo_media_id,
            title: m.title,
            media_type: m.media_type,
            year: m.year,
            overview: m.overview,
            poster_url: m.poster_url,
            backdrop_url: m.backdrop_url,
            genres: m.genres,
            rating: m.rating,
            watch_status: m.watch_status,
            score: m.score,
            is_favorite: m.is_favorite,
            availability: m.availability.into(),
        }
    }
}

/// A page of media items.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MediaSearchResponse {
    pub items: Vec<MediaItem>,
    pub next_cursor: Option<String>,
}

/// Full detail for one media item.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MediaDetailResponse {
    /// The base media item with the current user's watch state.
    pub media: MediaItem,
    pub tagline: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub status: Option<String>,
    pub homepage: Option<String>,
    pub imdb_id: Option<String>,
    pub number_of_seasons: Option<u32>,
    pub number_of_episodes: Option<u32>,
    pub in_production: Option<bool>,
}

impl From<MediaDetails> for MediaDetailResponse {
    fn from(d: MediaDetails) -> Self {
        Self {
            media: d.media.into(),
            tagline: d.tagline,
            runtime_minutes: d.runtime_minutes,
            status: d.status,
            homepage: d.homepage,
            imdb_id: d.imdb_id,
            number_of_seasons: d.number_of_seasons,
            number_of_episodes: d.number_of_episodes,
            in_production: d.in_production,
        }
    }
}

/// One season of a TV show.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Season {
    pub season_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub poster_url: Option<String>,
    pub episode_count: u32,
}

impl From<SeasonInfo> for Season {
    fn from(s: SeasonInfo) -> Self {
        Self {
            season_number: s.season_number,
            name: s.name,
            overview: s.overview,
            air_date: s.air_date,
            poster_url: s.poster_url,
            episode_count: s.episode_count,
        }
    }
}

/// The seasons of a TV show.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SeasonsResponse {
    pub seasons: Vec<Season>,
}

/// One episode of a TV show.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Episode {
    pub season_number: u32,
    pub episode_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub still_url: Option<String>,
    pub rating: Option<f64>,
}

impl From<EpisodeInfo> for Episode {
    fn from(e: EpisodeInfo) -> Self {
        Self {
            season_number: e.season_number,
            episode_number: e.episode_number,
            name: e.name,
            overview: e.overview,
            air_date: e.air_date,
            runtime_minutes: e.runtime_minutes,
            still_url: e.still_url,
            rating: e.rating,
        }
    }
}

/// The episodes of one season.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EpisodesResponse {
    pub episodes: Vec<Episode>,
}
