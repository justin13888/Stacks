use serde::{Deserialize, Serialize};

use crate::import_export::error::ImportExportError;
use crate::import_export::models::ImportEntry;

/// Raw entry as it appears in a MyAnimeList export XML/JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalEntry {
    pub mal_id: u64,
    pub title: String,
    pub status: Option<String>,
    pub score: Option<i32>,
    pub episodes_watched: Option<i32>,
}

/// Parse a MAL XML export file (bytes) into a list of `ImportEntry` values.
pub fn parse_mal_xml(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let _ = data;
    todo!()
}

/// Parse a MAL JSON export into a list of `ImportEntry` values.
pub fn parse_mal_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let _ = data;
    todo!()
}

/// Map a `MalEntry` to a generic `ImportEntry`.
pub fn map_entry(entry: MalEntry) -> ImportEntry {
    let _ = entry;
    todo!()
}
