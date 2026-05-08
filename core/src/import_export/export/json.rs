use crate::import_export::error::ImportExportError;
use crate::import_export::models::ImportEntry;

/// Serialize a list of `ImportEntry` values to a pretty-printed JSON byte vector.
pub fn export_json(entries: &[ImportEntry]) -> Result<Vec<u8>, ImportExportError> {
    serde_json::to_vec_pretty(entries)
        .map_err(|e| ImportExportError::Serialization(e.to_string()))
}
