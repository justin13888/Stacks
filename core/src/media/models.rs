use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};

use crate::entity::enums::{Category, EntryStatus, MediaFormat, ReleaseStatus};
use crate::entity::{episode, media, season};

use super::error::MetadataError;
use super::id::{MediaId, MediaKind};

/// A media item enriched with the current user's library state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaWithUserState {
    /// The public, self-describing id (`tmdb:movie:550`).
    pub cameo_media_id: String,
    pub title: String,
    /// The original-language title, when the provider gives one that
    /// differs from `title`.
    pub original_title: Option<String>,
    /// The romanised title (AniList's romaji), when the provider gives one.
    pub romaji_title: Option<String>,
    pub kind: MediaKind,
    /// The browsing category (movie, series, anime, k-drama, …).
    pub category: Category,
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    /// A show's total episode count, when known; `None` for a movie.
    pub episode_count: Option<u32>,
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
            original_title: m.original_title.clone(),
            romaji_title: m.romaji_title.clone(),
            kind: m.kind,
            category: m.category,
            year: m.start_date.map(|d| chrono::Datelike::year(&d)),
            overview: m.overview.clone(),
            poster_url: m.poster_url.clone(),
            backdrop_url: m.backdrop_url.clone(),
            genres: m.genres.clone(),
            rating: m.raw.as_ref().and_then(raw_vote_average),
            episode_count: show_episode_count(m),
            watch_status: None,
            rewatching: false,
            score: None,
            is_favorite: false,
        }
    }
}

/// A show's stored total episode count; `None` for a movie, or when unknown.
fn show_episode_count(m: &media::Model) -> Option<u32> {
    if m.kind != MediaKind::Show {
        return None;
    }
    m.episode_count.and_then(|c| u32::try_from(c).ok())
}

/// The provider rating kept inside a snapshot's `raw` payload, if any.
fn raw_vote_average(raw: &serde_json::Value) -> Option<f64> {
    ["movie", "show"]
        .iter()
        .find_map(|k| raw.get(k)?.get("vote_average")?.as_f64())
}

/// Whether a read came live from the provider or from the stored snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freshness {
    /// `true` when the provider could not be reached and the stored snapshot
    /// was served instead.
    pub stale: bool,
    /// When the data was last fetched from the provider, if ever.
    pub fetched_at: Option<DateTimeWithTimeZone>,
}

impl Freshness {
    /// Data fetched from the provider just now.
    pub fn live() -> Self {
        Self {
            stale: false,
            fetched_at: Some(crate::db::now()),
        }
    }

    /// Data served from a snapshot fetched at `fetched_at`.
    pub fn stale(fetched_at: Option<DateTimeWithTimeZone>) -> Self {
        Self {
            stale: true,
            fetched_at,
        }
    }
}

/// Full detail for a single media item — a [`MediaWithUserState`] plus the
/// extended metadata shown on a detail page.
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaDetails {
    /// The base media item together with the current user's library state.
    pub media: MediaWithUserState,
    /// The release format (tv, tv_short, movie, ova, ona, special,
    /// miniseries), when known.
    pub format: Option<MediaFormat>,
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
    /// Number of seasons (shows only). The episode count is
    /// [`MediaWithUserState::episode_count`].
    pub number_of_seasons: Option<u32>,
    /// Whether the show is still in production (shows only), derived from its
    /// release status; `None` when the status is unknown.
    pub in_production: Option<bool>,
    /// Whether this came live from the provider or from the snapshot.
    pub freshness: Freshness,
}

impl MediaDetails {
    /// Detail rebuilt from a stored snapshot, for when the provider cannot be
    /// reached. The fields the snapshot has no column for come from the
    /// provider payload kept in `raw`.
    pub fn from_snapshot(m: &media::Model, public_id: &MediaId) -> Self {
        let raw_str = |key: &str| {
            m.raw
                .as_ref()
                .and_then(|r| r.get(key))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let is_show = m.kind == MediaKind::Show;
        Self {
            media: MediaWithUserState::from_snapshot(m, public_id),
            format: m.format,
            tagline: raw_str("tagline"),
            runtime_minutes: m.runtime_minutes.and_then(|r| u32::try_from(r).ok()),
            status: raw_str("status"),
            homepage: raw_str("homepage"),
            imdb_id: raw_str("imdb_id"),
            number_of_seasons: if is_show {
                m.season_count.and_then(|c| u32::try_from(c).ok())
            } else {
                None
            },
            in_production: if is_show {
                in_production(m.status)
            } else {
                None
            },
            freshness: Freshness::stale(m.fetched_at),
        }
    }
}

/// Whether a show with this release status is still being made. Derived
/// from the status rather than trusted from a provider flag: cameo defaults
/// TMDB's `in_production` to `false` when the field is missing.
pub fn in_production(status: Option<ReleaseStatus>) -> Option<bool> {
    Some(match status? {
        ReleaseStatus::Upcoming | ReleaseStatus::Releasing | ReleaseStatus::Hiatus => true,
        ReleaseStatus::Finished | ReleaseStatus::Cancelled => false,
    })
}

/// Summary of one season of a show.
#[derive(Debug, Serialize, Deserialize)]
pub struct SeasonInfo {
    pub season_number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub poster_url: Option<String>,
    /// `None` when the provider did not say.
    pub episode_count: Option<u32>,
}

impl SeasonInfo {
    /// A season summary from its stored snapshot row.
    pub fn from_snapshot(s: &season::Model) -> Self {
        Self {
            season_number: u32::try_from(s.number).unwrap_or_default(),
            name: s.title.clone(),
            overview: s.overview.clone(),
            air_date: s.air_date.map(|d| d.to_string()),
            poster_url: s.poster_url.clone(),
            episode_count: s.episode_count.and_then(|c| u32::try_from(c).ok()),
        }
    }
}

/// Summary of one episode of a show.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeInfo {
    pub season_number: u32,
    pub episode_number: u32,
    /// The episode's number counted across the whole show, when known.
    pub absolute_number: Option<u32>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub still_url: Option<String>,
    pub rating: Option<f64>,
}

impl EpisodeInfo {
    /// An episode summary from its stored snapshot row. The snapshot keeps
    /// no rating.
    pub fn from_snapshot(season_number: u32, e: &episode::Model) -> Self {
        Self {
            season_number,
            episode_number: u32::try_from(e.number).unwrap_or_default(),
            absolute_number: e.absolute_number.and_then(|n| u32::try_from(n).ok()),
            name: e.title.clone(),
            overview: e.overview.clone(),
            air_date: e.air_date.map(|d| d.to_string()),
            runtime_minutes: e.runtime_minutes.and_then(|r| u32::try_from(r).ok()),
            still_url: e.still_url.clone(),
            rating: None,
        }
    }
}

/// The seasons of a show, live or from the snapshot.
#[derive(Debug, Serialize, Deserialize)]
pub struct SeasonList {
    pub seasons: Vec<SeasonInfo>,
    pub freshness: Freshness,
}

/// The episodes of one season, live or from the snapshot.
#[derive(Debug, Serialize, Deserialize)]
pub struct EpisodeList {
    pub episodes: Vec<EpisodeInfo>,
    pub freshness: Freshness,
}

/// A page of results assembled from one or more provider calls.
///
/// A listing never hides a failure: `degraded` names every provider call that
/// failed (the rest of the page still comes back), and `unavailable` names
/// every upstream entry that could not be turned into an item.
#[derive(Debug)]
pub struct Listing<T> {
    pub items: Vec<T>,
    /// The provider page that follows this one, when there is one.
    pub next_page: Option<u32>,
    /// Upstream refs (as the provider gave them) that could not be mapped.
    pub unavailable: Vec<String>,
    /// The provider calls that failed while assembling this listing.
    pub degraded: Vec<MetadataError>,
}

impl<T> Listing<T> {
    /// Whether anything is missing from this listing.
    pub fn is_partial(&self) -> bool {
        !self.unavailable.is_empty() || !self.degraded.is_empty()
    }
}
