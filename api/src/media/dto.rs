use salvo::oapi::{ToParameters, ToSchema};
use serde::{Deserialize, Serialize};
use stacks_core::entity::enums::EntryStatus;
use stacks_core::media::MediaKind;
use stacks_core::media::models::{EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};

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

/// Streaming availability of a media item.
///
/// The streaming-server integration is removed until the media server
/// provides what #13 lists, so this is always "not available". The field is
/// kept so the current web client keeps compiling; the API redesign drops it.
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct Availability {
    pub available: bool,
    pub servers: Vec<AvailabilityServer>,
}

/// The status vocabulary this API version speaks (`plan_to_watch`,
/// `on_hold`, `rewatching`, …), mapped from the library's statuses.
pub fn legacy_status(status: Option<EntryStatus>, rewatching: bool) -> Option<String> {
    Some(
        match status? {
            EntryStatus::Planning => "plan_to_watch",
            EntryStatus::Watching if rewatching => "rewatching",
            EntryStatus::Watching => "watching",
            EntryStatus::Completed => "completed",
            EntryStatus::Paused => "on_hold",
            EntryStatus::Dropped => "dropped",
        }
        .to_owned(),
    )
}

/// Parse either vocabulary into a library status. `rewatching` maps to
/// `watching`; the caller starts the rewatch run.
pub fn parse_status(raw: &str) -> Option<EntryStatus> {
    match raw {
        "plan_to_watch" => Some(EntryStatus::Planning),
        "on_hold" => Some(EntryStatus::Paused),
        "rewatching" => Some(EntryStatus::Watching),
        other => EntryStatus::parse(other),
    }
}

/// The `media_type` token this API version speaks.
pub fn legacy_media_type(kind: MediaKind) -> String {
    match kind {
        MediaKind::Movie => "movie",
        MediaKind::Show => "tv",
    }
    .to_owned()
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
            media_type: legacy_media_type(m.kind),
            year: m.year,
            overview: m.overview,
            poster_url: m.poster_url,
            backdrop_url: m.backdrop_url,
            genres: m.genres,
            rating: m.rating,
            watch_status: legacy_status(m.watch_status, m.rewatching),
            score: m.score,
            is_favorite: m.is_favorite,
            availability: Availability::default(),
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
            episode_count: s.episode_count.unwrap_or_default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use stacks_core::entity::enums::EntryStatus::*;

    #[test]
    fn legacy_status_round_trips_through_parse() {
        for s in [Planning, Watching, Completed, Paused, Dropped] {
            let wire = legacy_status(Some(s), false).unwrap();
            assert_eq!(parse_status(&wire), Some(s), "{wire}");
        }
        assert_eq!(
            legacy_status(Some(Watching), true).as_deref(),
            Some("rewatching")
        );
        assert_eq!(parse_status("rewatching"), Some(Watching));
        assert_eq!(legacy_status(None, false), None);
    }

    #[test]
    fn both_status_vocabularies_parse() {
        assert_eq!(parse_status("plan_to_watch"), Some(Planning));
        assert_eq!(parse_status("planning"), Some(Planning));
        assert_eq!(parse_status("on_hold"), Some(Paused));
        assert_eq!(parse_status("paused"), Some(Paused));
        assert_eq!(parse_status("binging"), None);
    }

    #[test]
    fn media_item_speaks_the_legacy_vocabulary() {
        let item = MediaItem::from(MediaWithUserState {
            cameo_media_id: "tmdb:show:1".into(),
            title: "T".into(),
            kind: MediaKind::Show,
            year: None,
            overview: None,
            poster_url: None,
            backdrop_url: None,
            genres: vec![],
            rating: None,
            watch_status: Some(Paused),
            rewatching: false,
            score: Some(50),
            is_favorite: true,
        });
        assert_eq!(item.media_type, "tv");
        assert_eq!(item.watch_status.as_deref(), Some("on_hold"));
        assert!(!item.availability.available);
        assert_eq!(legacy_media_type(MediaKind::Movie), "movie");
    }
}
