use serde::{Deserialize, Serialize};

use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ExternalIds, ImportEntry};

/// Raw entry as returned by TMDB list / rated-movies endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbEntry {
    pub tmdb_id: u64,
    pub title: String,
    #[serde(default)]
    pub media_type: String,
    pub status: Option<String>,
    pub rating: Option<f64>,
}

/// Parse a TMDB exported JSON list (an array of `TmdbEntry`) into
/// `ImportEntry` values.
pub fn parse_tmdb_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let raw: Vec<TmdbEntry> =
        serde_json::from_slice(data).map_err(|e| ImportExportError::ParseError(e.to_string()))?;
    Ok(raw.into_iter().map(map_entry).collect())
}

/// Map a `TmdbEntry` to a generic `ImportEntry`.
pub fn map_entry(entry: TmdbEntry) -> ImportEntry {
    ImportEntry {
        external_ids: ExternalIds {
            tmdb_id: Some(entry.tmdb_id.to_string()),
            ..Default::default()
        },
        status: entry.status,
        score: entry.rating.map(|r| r.round() as i32),
        progress: None,
        started_at: None,
        completed_at: None,
        source: "tmdb".to_string(),
    }
}
