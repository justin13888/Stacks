use crate::import_export::error::ImportExportError;
use crate::import_export::models::ImportEntry;

/// Serialize a slice of [`ImportEntry`] values to pretty-printed JSON bytes.
pub fn export_json(entries: &[ImportEntry]) -> Result<Vec<u8>, ImportExportError> {
    serde_json::to_vec_pretty(entries)
        .map_err(|e| ImportExportError::Serialization(format!("JSON: {e}")))
}
