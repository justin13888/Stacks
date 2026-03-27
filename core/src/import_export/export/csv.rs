use crate::import_export::error::ImportExportError;
use crate::import_export::models::ImportEntry;

/// Serialize a list of `ImportEntry` values to CSV bytes.
pub fn export_csv(entries: &[ImportEntry]) -> Result<Vec<u8>, ImportExportError> {
    let _ = entries;
    todo!()
}
