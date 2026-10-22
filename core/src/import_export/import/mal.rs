use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::import_export::error::ImportExportError;
use crate::import_export::models::{
    ExternalIds, ImportEntry, normalize_status, scale_ten_to_hundred,
};

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
        // MAL scores out of 10; Stacks stores 0-100.
        score: entry.score.map(|s| scale_ten_to_hundred(s as f64)),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic MyAnimeList XML export, trimmed to the fields Stacks reads.
    const MAL_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<myanimelist>
  <myinfo>
    <user_id>123456</user_id>
    <user_name>tester</user_name>
  </myinfo>
  <anime>
    <series_animedb_id>30</series_animedb_id>
    <series_title><![CDATA[Neon Genesis Evangelion]]></series_title>
    <my_watched_episodes>26</my_watched_episodes>
    <my_start_date>2024-01-05</my_start_date>
    <my_finish_date>2024-02-11</my_finish_date>
    <my_score>10</my_score>
    <my_status>Completed</my_status>
  </anime>
  <anime>
    <series_animedb_id>21</series_animedb_id>
    <series_title><![CDATA[One Piece]]></series_title>
    <my_watched_episodes>1000</my_watched_episodes>
    <my_start_date>0000-00-00</my_start_date>
    <my_finish_date>0000-00-00</my_finish_date>
    <my_score>0</my_score>
    <my_status>Watching</my_status>
  </anime>
</myanimelist>
"#;

    #[test]
    fn parses_a_mal_xml_export() {
        let entries = parse_mal_xml(MAL_XML.as_bytes()).expect("parses");
        assert_eq!(entries.len(), 2);

        let eva = &entries[0];
        assert_eq!(eva.external_ids.mal_id.as_deref(), Some("30"));
        assert_eq!(eva.status.as_deref(), Some("completed"));
        // MAL scores out of 10; Stacks stores 0-100.
        assert_eq!(eva.score, Some(100));
        assert_eq!(eva.progress, Some(26));
        assert!(eva.started_at.is_some());
        assert!(eva.completed_at.is_some());
        assert_eq!(eva.source, "mal");

        let op = &entries[1];
        assert_eq!(op.external_ids.mal_id.as_deref(), Some("21"));
        assert_eq!(op.status.as_deref(), Some("watching"));
        assert_eq!(op.score, Some(0));
        // MAL writes 0000-00-00 for "unset"; it must not become a real date.
        assert!(op.started_at.is_none());
        assert!(op.completed_at.is_none());
    }

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
    fn parses_a_mal_json_export() {
        let json = br#"[{"mal_id":30,"title":"Evangelion","status":"completed","score":9,
                         "episodes_watched":26}]"#;
        let entries = parse_mal_json(json).expect("parses");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].external_ids.mal_id.as_deref(), Some("30"));
        assert_eq!(entries[0].score, Some(90));
    }
}
