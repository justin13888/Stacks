use serde::{Deserialize, Serialize};

use crate::import_export::error::ImportExportError;
use crate::import_export::models::ImportEntry;

/// Raw entry as returned by TMDB list / rated-movies endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbEntry {
    pub tmdb_id: u64,
    pub title: String,
    pub media_type: String,
    pub status: Option<String>,
    pub rating: Option<f64>,
}

/// Parse a TMDB exported JSON list into a list of `ImportEntry` values.
pub fn parse_tmdb_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let _ = data;
    todo!()
}

/// Map a `TmdbEntry` to a generic `ImportEntry`.
pub fn map_entry(entry: TmdbEntry) -> ImportEntry {
    let _ = entry;
    todo!()
}
