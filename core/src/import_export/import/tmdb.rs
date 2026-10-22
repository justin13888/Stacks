use serde::{Deserialize, Serialize};

use crate::import_export::error::ImportExportError;
use crate::import_export::models::{
    ExternalIds, ImportEntry, normalize_status, scale_ten_to_hundred,
};

/// One entry in a TMDB-style JSON export. The shape is intentionally loose:
/// `media_type` ("movie"/"tv") and `tmdb_id` are the required fields.
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

/// Parse a TMDB JSON export — a JSON array of [`TmdbEntry`].
pub fn parse_tmdb_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let entries: Vec<TmdbEntry> = serde_json::from_slice(data)
        .map_err(|e| ImportExportError::ParseError(format!("TMDB JSON: {e}")))?;
    Ok(entries.into_iter().map(map_entry).collect())
}

/// Map a [`TmdbEntry`] to a generic [`ImportEntry`].
///
/// `media_type` is folded into the source string (`tmdb:movie` / `tmdb:tv`) so
/// the import pipeline can disambiguate movie vs TV when resolving the id.
pub fn map_entry(entry: TmdbEntry) -> ImportEntry {
    let _ = entry.title;
    let source = match entry.media_type.to_ascii_lowercase().as_str() {
        "tv" | "show" | "series" => "tmdb:tv".to_string(),
        _ => "tmdb:movie".to_string(),
    };
    ImportEntry {
        external_ids: ExternalIds {
            tmdb_id: Some(entry.tmdb_id.to_string()),
            ..Default::default()
        },
        status: entry.status.as_deref().map(normalize_status),
        // TMDB rates out of 10; Stacks stores 0-100.
        score: entry.rating.map(scale_ten_to_hundred),
        progress: None,
        started_at: None,
        completed_at: None,
        source,
    }
}
