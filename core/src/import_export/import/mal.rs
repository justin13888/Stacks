//! MyAnimeList exports.
//!
//! - **MAL XML** is the file MAL's own "Export" page produces
//!   (`animelist_*.xml`, after `gunzip`).
//! - **MAL JSON** is either MAL API v2's anime list
//!   (`GET /v2/users/{user}/animelist?fields=list_status,num_episodes,media_type,start_date`,
//!   one page `{"data": [...]}`, several pages as an array, or the bare
//!   `data` array), or the loose array
//!   `[{"mal_id", "title", "status", "score", "episodes_watched",
//!   "started_at", "completed_at"}]` earlier Stacks releases read.
//!
//! Every field MAL provides that Stacks keeps is read: the status, the score
//! (MAL's 0 is "no score"), the episodes watched, the start and finish dates
//! with their precision (MAL writes `0000-00-00` for none and zeroes for an
//! unknown month or day), the rewatch count, whether a rewatch is under way,
//! and the comments.

use serde::{Deserialize, Serialize};

use crate::entity::enums::MediaKind;
use crate::import_export::error::ImportExportError;
use crate::import_export::models::{
    ExternalIds, ImportDate, ImportEntry, normalize_status, parse_partial_date,
    scale_ten_to_hundred,
};

/// Root element of a `<myanimelist>` XML export.
#[derive(Debug, Deserialize)]
struct MalXmlExport {
    #[serde(rename = "anime", default)]
    anime: Vec<MalXmlAnime>,
}

#[derive(Debug, Deserialize)]
struct MalXmlAnime {
    series_animedb_id: u64,
    #[serde(default)]
    series_title: Option<String>,
    /// `TV`, `Movie`, `OVA`, `ONA`, `Special`, `Music`, `Unknown`.
    #[serde(default)]
    series_type: Option<String>,
    #[serde(default)]
    series_episodes: Option<String>,
    #[serde(default)]
    my_watched_episodes: Option<String>,
    #[serde(default)]
    my_start_date: Option<String>,
    #[serde(default)]
    my_finish_date: Option<String>,
    #[serde(default)]
    my_score: Option<String>,
    #[serde(default)]
    my_status: Option<String>,
    #[serde(default)]
    my_comments: Option<String>,
    #[serde(default)]
    my_times_watched: Option<String>,
    #[serde(default)]
    my_rewatching: Option<String>,
}

/// One entry of the loose JSON array earlier releases read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalEntry {
    pub mal_id: u64,
    #[serde(default)]
    pub title: Option<String>,
    pub status: Option<String>,
    pub score: Option<i32>,
    pub episodes_watched: Option<i32>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
}

/// One item of MAL API v2's anime list.
#[derive(Debug, Deserialize)]
struct ApiItem {
    node: ApiNode,
    #[serde(default)]
    list_status: Option<ApiListStatus>,
}

#[derive(Debug, Deserialize)]
struct ApiNode {
    id: u64,
    #[serde(default)]
    title: Option<String>,
    /// `tv`, `movie`, `ova`, `ona`, `special`, `music`, `unknown`.
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    num_episodes: Option<i32>,
    #[serde(default)]
    start_date: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ApiListStatus {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    score: Option<i32>,
    #[serde(default)]
    num_episodes_watched: Option<i32>,
    #[serde(default)]
    is_rewatching: Option<bool>,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    finish_date: Option<String>,
    #[serde(default)]
    num_times_rewatched: Option<i32>,
    #[serde(default)]
    comments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiPage {
    data: Vec<ApiItem>,
}

/// Every JSON shape the importer reads.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MalJson {
    Page(ApiPage),
    Pages(Vec<ApiPage>),
    Items(Vec<ApiItem>),
    Loose(Vec<MalEntry>),
}

/// Parse a MAL XML export. Gzipped uploads are rejected with a clear message —
/// decompress with `gunzip` first.
pub fn parse_mal_xml(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    if data.starts_with(b"\x1f\x8b") {
        return Err(ImportExportError::ParseError(
            "MAL export appears to be gzipped — decompress it before uploading".to_string(),
        ));
    }
    let xml = std::str::from_utf8(data)
        .map_err(|e| ImportExportError::ParseError(format!("MAL XML: {e}")))?;
    let export: MalXmlExport = quick_xml::de::from_str(xml)
        .map_err(|e| ImportExportError::ParseError(format!("MAL XML: {e}")))?;
    Ok(export.anime.into_iter().map(xml_entry).collect())
}

fn xml_entry(a: MalXmlAnime) -> ImportEntry {
    let int = |v: &Option<String>| v.as_deref().and_then(|s| s.trim().parse::<i32>().ok());
    ImportEntry {
        source: "mal".into(),
        external_ids: mal_id(a.series_animedb_id),
        title: text(a.series_title),
        kind: a.series_type.as_deref().and_then(kind),
        year: None,
        total_episodes: int(&a.series_episodes).filter(|n| *n > 0),
        status: text(a.my_status).as_deref().map(normalize_status),
        score: score(int(&a.my_score)),
        progress: int(&a.my_watched_episodes).filter(|n| *n >= 0),
        rewatch_count: int(&a.my_times_watched).filter(|n| *n > 0),
        rewatching: a
            .my_rewatching
            .as_deref()
            .is_some_and(|r| matches!(r.trim(), "1" | "true" | "yes")),
        started: date(a.my_start_date.as_deref()),
        completed: date(a.my_finish_date.as_deref()),
        notes: text(a.my_comments),
        favourite: None,
    }
}

/// Parse a MAL JSON export, in any shape [the module](self) lists.
pub fn parse_mal_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let parsed: MalJson = serde_json::from_slice(data).map_err(|e| {
        ImportExportError::ParseError(format!(
            "MAL JSON: not a MAL API anime list or a Stacks MAL array ({e})"
        ))
    })?;
    Ok(match parsed {
        MalJson::Page(p) => p.data.into_iter().map(api_entry).collect(),
        MalJson::Pages(pages) => pages
            .into_iter()
            .flat_map(|p| p.data)
            .map(api_entry)
            .collect(),
        MalJson::Items(items) => items.into_iter().map(api_entry).collect(),
        MalJson::Loose(entries) => entries.into_iter().map(map_entry).collect(),
    })
}

fn api_entry(item: ApiItem) -> ImportEntry {
    let node = item.node;
    let s = item.list_status.unwrap_or_default();
    ImportEntry {
        source: "mal".into(),
        external_ids: mal_id(node.id),
        title: text(node.title),
        kind: node.media_type.as_deref().and_then(kind),
        year: node
            .start_date
            .as_deref()
            .and_then(parse_partial_date)
            .map(|d| chrono::Datelike::year(&d.date)),
        total_episodes: node.num_episodes.filter(|n| *n > 0),
        status: s.status.as_deref().map(normalize_status),
        score: score(s.score),
        progress: s.num_episodes_watched.filter(|n| *n >= 0),
        rewatch_count: s.num_times_rewatched.filter(|n| *n > 0),
        rewatching: s.is_rewatching.unwrap_or(false),
        started: date(s.start_date.as_deref()),
        completed: date(s.finish_date.as_deref()),
        notes: text(s.comments),
        favourite: None,
    }
}

/// Map a loose [`MalEntry`] to an [`ImportEntry`].
pub fn map_entry(entry: MalEntry) -> ImportEntry {
    ImportEntry {
        source: "mal".into(),
        external_ids: mal_id(entry.mal_id),
        title: text(entry.title),
        status: entry.status.as_deref().map(normalize_status),
        score: score(entry.score),
        progress: entry.episodes_watched.filter(|n| *n >= 0),
        started: date(entry.started_at.as_deref()),
        completed: date(entry.completed_at.as_deref()),
        ..Default::default()
    }
}

fn mal_id(id: u64) -> ExternalIds {
    ExternalIds {
        mal_id: Some(id.to_string()),
        ..Default::default()
    }
}

/// A MAL score (1–10, 0 for none) on Stacks' 0–100 scale.
fn score(raw: Option<i32>) -> Option<i32> {
    raw.filter(|s| (1..=10).contains(s))
        .map(|s| scale_ten_to_hundred(f64::from(s)))
}

fn date(raw: Option<&str>) -> Option<ImportDate> {
    raw.and_then(parse_partial_date)
}

fn text(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// A MAL media type as a Stacks kind: only a movie is a movie.
fn kind(raw: &str) -> Option<MediaKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "movie" => Some(MediaKind::Movie),
        "tv" | "ova" | "ona" | "special" | "tv_special" | "music" | "cm" | "pv" => {
            Some(MediaKind::Show)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::enums::DatePrecision;

    #[test]
    fn an_empty_export_is_not_an_error() {
        let entries =
            parse_mal_xml(br#"<?xml version="1.0"?><myanimelist></myanimelist>"#).expect("parses");
        assert!(entries.is_empty());
    }

    #[test]
    fn gzipped_uploads_are_rejected_with_guidance() {
        let err = parse_mal_xml(&[0x1f, 0x8b, 0x08, 0x00]).unwrap_err();
        assert!(err.to_string().contains("gzipped"));
    }

    #[test]
    fn malformed_xml_is_an_error_not_a_panic() {
        assert!(parse_mal_xml(b"<myanimelist><anime>").is_err());
        assert!(parse_mal_xml(b"not xml at all").is_err());
    }

    #[test]
    fn a_zero_score_is_no_score_and_a_partial_date_keeps_its_precision() {
        let xml = br#"<myanimelist><anime>
            <series_animedb_id>1</series_animedb_id>
            <my_score>0</my_score>
            <my_start_date>2019-04-00</my_start_date>
            <my_finish_date>0000-00-00</my_finish_date>
            <my_status>On-Hold</my_status>
        </anime></myanimelist>"#;
        let [e] = parse_mal_xml(xml).unwrap().try_into().unwrap();
        assert_eq!(e.score, None);
        assert_eq!(e.status.as_deref(), Some("paused"));
        assert_eq!(e.started.map(|d| d.precision), Some(DatePrecision::Month));
        assert_eq!(e.completed, None);
    }

    #[test]
    fn the_loose_array_still_reads() {
        let json = br#"[{"mal_id":30,"title":"Evangelion","status":"completed","score":9,
                         "episodes_watched":26}]"#;
        let entries = parse_mal_json(json).expect("parses");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].external_ids.mal_id.as_deref(), Some("30"));
        assert_eq!(entries[0].score, Some(90));
        assert_eq!(entries[0].title.as_deref(), Some("Evangelion"));
    }

    #[test]
    fn json_that_is_no_mal_shape_is_refused() {
        assert!(parse_mal_json(br#"{"hello": "world"}"#).is_err());
        assert!(parse_mal_json(b"<xml/>").is_err());
        assert!(parse_mal_json(b"[]").unwrap().is_empty());
    }
}
