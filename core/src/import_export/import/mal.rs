use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ExternalIds, ImportEntry, normalize_status};

/// One entry as it appears in a MAL JSON export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalEntry {
    pub mal_id: u64,
    pub title: String,
    pub status: Option<String>,
    pub score: Option<i32>,
    pub episodes_watched: Option<i32>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
}

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
    series_title: String,
    #[serde(default)]
    my_status: Option<String>,
    #[serde(default)]
    my_score: Option<i32>,
    #[serde(default)]
    my_watched_episodes: Option<i32>,
    #[serde(default)]
    my_start_date: Option<String>,
    #[serde(default)]
    my_finish_date: Option<String>,
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
    Ok(export
        .anime
        .into_iter()
        .map(|a| {
            map_entry(MalEntry {
                mal_id: a.series_animedb_id,
                title: a.series_title,
                status: a.my_status,
                score: a.my_score,
                episodes_watched: a.my_watched_episodes,
                started_at: a.my_start_date,
                completed_at: a.my_finish_date,
            })
        })
        .collect())
}

/// Parse a MAL JSON export — a JSON array of [`MalEntry`].
pub fn parse_mal_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let entries: Vec<MalEntry> = serde_json::from_slice(data)
        .map_err(|e| ImportExportError::ParseError(format!("MAL JSON: {e}")))?;
    Ok(entries.into_iter().map(map_entry).collect())
}

/// Map a [`MalEntry`] to a generic [`ImportEntry`].
pub fn map_entry(entry: MalEntry) -> ImportEntry {
    let _ = entry.title; // title is captured by cameo when the id is resolved
    ImportEntry {
        external_ids: ExternalIds {
            mal_id: Some(entry.mal_id.to_string()),
            ..Default::default()
        },
        status: entry.status.as_deref().map(normalize_status),
        score: entry.score,
        progress: entry.episodes_watched,
        started_at: entry.started_at.as_deref().and_then(parse_date),
        completed_at: entry.completed_at.as_deref().and_then(parse_date),
        source: "mal".to_string(),
    }
}

/// Parse a `YYYY-MM-DD` date as midnight UTC. MAL sometimes emits `0000-00-00`
/// for "unset" — those parse as `None`.
fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc())
}
