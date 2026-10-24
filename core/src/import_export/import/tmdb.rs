//! TMDB exports, as JSON.
//!
//! Three shapes are read:
//!
//! 1. **An account dump**: an object keyed by the TMDB account endpoint each
//!    list came from — `rated/movies`, `rated/tv`, `watchlist/movies`,
//!    `watchlist/tv`, `favorite/movies`, `favorite/tv` — each holding that
//!    endpoint's response page, or an array of its pages. This is what
//!    saving the responses of TMDB's v3 (`/account/{id}/rated/movies`, …)
//!    or v4 (`/account/{id}/movie/rated`, …) account API produces.
//! 2. **Response pages on their own**: one `{"page", "results": [...]}`
//!    object, or an array of them. Without the endpoint's name, a result
//!    with a rating is read as rated and one without as on the watchlist.
//! 3. **The loose array** earlier Stacks releases read:
//!    `[{"tmdb_id", "title", "media_type", "status", "rating"}]`.
//!
//! A result names its kind by `media_type`, or by having a `title` (a movie)
//! or a `name` (a show). Ratings (v3 `rating`, v4 `account_rating.value`)
//! are out of 10 and become 0–100 scores. A rated title is `completed`, a
//! watchlisted one `planning`, and a favourite carries the favourite flag;
//! a title on several lists is one row combining them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entity::enums::{EntryStatus, MediaKind};
use crate::import_export::error::ImportExportError;
use crate::import_export::models::{
    ExternalIds, ImportEntry, normalize_status, parse_partial_date, scale_ten_to_hundred,
};

/// One entry of the loose array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbEntry {
    pub tmdb_id: u64,
    pub title: String,
    /// "movie" or "tv".
    pub media_type: String,
    pub status: Option<String>,
    /// TMDB-style rating, 0–10.
    pub rating: Option<f64>,
}

/// One result of a TMDB list endpoint.
#[derive(Debug, Deserialize)]
struct Item {
    id: u64,
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    first_air_date: Option<String>,
    /// v3's rated endpoints.
    #[serde(default)]
    rating: Option<f64>,
    /// v4's rated endpoints.
    #[serde(default)]
    account_rating: Option<AccountRating>,
}

#[derive(Debug, Deserialize)]
struct AccountRating {
    value: f64,
}

#[derive(Debug, Deserialize)]
struct Page {
    results: Vec<Item>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Pages {
    One(Page),
    Many(Vec<Page>),
}

impl Pages {
    fn results(self) -> Vec<Item> {
        match self {
            Pages::One(p) => p.results,
            Pages::Many(ps) => ps.into_iter().flat_map(|p| p.results).collect(),
        }
    }
}

/// Tried in order: a page has `results`, which no dump key is.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TmdbJson {
    Pages(Pages),
    Dump(BTreeMap<String, Pages>),
    Loose(Vec<TmdbEntry>),
}

/// The list a result came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum List {
    Rated,
    Watchlist,
    Favourite,
}

/// The endpoint names an account dump is keyed by, and what each one is.
const ENDPOINTS: &[(&str, List, MediaKind)] = &[
    ("rated/movies", List::Rated, MediaKind::Movie),
    ("rated/tv", List::Rated, MediaKind::Show),
    ("watchlist/movies", List::Watchlist, MediaKind::Movie),
    ("watchlist/tv", List::Watchlist, MediaKind::Show),
    ("favorite/movies", List::Favourite, MediaKind::Movie),
    ("favorite/tv", List::Favourite, MediaKind::Show),
];

/// Parse a TMDB JSON export, in any shape [the module](self) lists.
pub fn parse_tmdb_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let parsed: TmdbJson = serde_json::from_slice(data).map_err(|e| {
        ImportExportError::ParseError(format!(
            "TMDB JSON: not a TMDB account dump, response page or Stacks TMDB array ({e})"
        ))
    })?;
    let mut rows = Rows::default();
    match parsed {
        TmdbJson::Loose(entries) => return Ok(entries.into_iter().map(map_entry).collect()),
        TmdbJson::Pages(pages) => {
            for r in pages.results() {
                let list = if rating(&r).is_some() {
                    List::Rated
                } else {
                    List::Watchlist
                };
                rows.add(r, list, None);
            }
        }
        TmdbJson::Dump(lists) => {
            for (key, pages) in lists {
                let Some((_, list, kind)) = ENDPOINTS.iter().find(|(k, ..)| *k == key) else {
                    return Err(ImportExportError::ParseError(format!(
                        "TMDB JSON: unknown list {key:?}; expected one of {}",
                        ENDPOINTS
                            .iter()
                            .map(|(k, ..)| *k)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                };
                for r in pages.results() {
                    rows.add(r, *list, Some(*kind));
                }
            }
        }
    }
    Ok(rows.0.into_values().collect())
}

fn rating(r: &Item) -> Option<f64> {
    r.account_rating.as_ref().map(|a| a.value).or(r.rating)
}

/// Rows keyed by title, so a title on several lists is one row.
#[derive(Default)]
struct Rows(BTreeMap<(String, u64), ImportEntry>);

impl Rows {
    fn add(&mut self, r: Item, list: List, kind: Option<MediaKind>) {
        let kind = kind
            .or(match r.media_type.as_deref() {
                Some("movie") => Some(MediaKind::Movie),
                Some("tv") => Some(MediaKind::Show),
                _ => None,
            })
            .or(if r.title.is_some() {
                Some(MediaKind::Movie)
            } else if r.name.is_some() {
                Some(MediaKind::Show)
            } else {
                None
            });
        let score = rating(&r).map(scale_ten_to_hundred);
        let year = r
            .release_date
            .as_deref()
            .or(r.first_air_date.as_deref())
            .and_then(parse_partial_date)
            .map(|d| chrono::Datelike::year(&d.date));
        let key = (kind.map_or("unknown", MediaKind::as_str).to_owned(), r.id);
        let row = self.0.entry(key).or_insert_with(|| ImportEntry {
            source: "tmdb".into(),
            external_ids: ExternalIds {
                tmdb_id: Some(r.id.to_string()),
                ..Default::default()
            },
            title: r.title.or(r.name),
            kind,
            year,
            ..Default::default()
        });
        match list {
            List::Rated => {
                row.status = Some(EntryStatus::Completed.as_str().to_owned());
                row.score = score.or(row.score);
            }
            List::Watchlist => {
                if row.status.is_none() {
                    row.status = Some(EntryStatus::Planning.as_str().to_owned());
                }
            }
            List::Favourite => row.favourite = Some(true),
        }
    }
}

/// Map a loose [`TmdbEntry`] to an [`ImportEntry`].
pub fn map_entry(entry: TmdbEntry) -> ImportEntry {
    let kind = match entry.media_type.to_ascii_lowercase().as_str() {
        "tv" | "show" | "series" => MediaKind::Show,
        _ => MediaKind::Movie,
    };
    ImportEntry {
        source: "tmdb".into(),
        external_ids: ExternalIds {
            tmdb_id: Some(entry.tmdb_id.to_string()),
            ..Default::default()
        },
        title: Some(entry.title).filter(|t| !t.trim().is_empty()),
        kind: Some(kind),
        status: entry.status.as_deref().map(normalize_status),
        // TMDB rates out of 10; Stacks stores 0-100.
        score: entry.rating.map(scale_ten_to_hundred),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_list_is_named_in_the_refusal() {
        let err = parse_tmdb_json(br#"{"seen/movies": {"page": 1, "results": []}}"#).unwrap_err();
        assert!(err.to_string().contains("seen/movies"));
    }

    #[test]
    fn json_that_is_no_tmdb_shape_is_refused() {
        assert!(parse_tmdb_json(b"<myanimelist/>").is_err());
        assert!(parse_tmdb_json(br#"[{"mal_id": 1}]"#).is_err());
        assert!(parse_tmdb_json(b"[]").unwrap().is_empty());
    }
}
