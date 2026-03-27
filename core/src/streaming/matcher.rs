use super::error::StreamingError;
use super::models::ConnectorLibraryItem;

/// Attempts to match a `ConnectorLibraryItem` to a canonical Cameo media ID
/// using external ID lookups and title/year heuristics.
pub async fn match_library_item(
    item: &ConnectorLibraryItem,
) -> Result<Option<String>, StreamingError> {
    let _ = item;
    todo!()
}

/// Batch-match a slice of library items, returning a vec of
/// `(item, Option<cameo_media_id>)` tuples.
pub async fn match_library_items(
    items: &[ConnectorLibraryItem],
) -> Result<Vec<(ConnectorLibraryItem, Option<String>)>, StreamingError> {
    let _ = items;
    todo!()
}
