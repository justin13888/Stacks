use super::error::StreamingError;
use super::models::ConnectorLibraryItem;

/// Attempts to match a `ConnectorLibraryItem` to a canonical Cameo media ID
/// using external ID lookups. Today this only handles TMDB IDs that the
/// connector has already populated in `external_ids`. Title/year fallback
/// will be added when search is integrated end-to-end.
pub async fn match_library_item(
    item: &ConnectorLibraryItem,
) -> Result<Option<String>, StreamingError> {
    if let Some(ext) = &item.external_ids {
        if let Some(tmdb) = ext.get("tmdb_id").and_then(|v| v.as_i64()) {
            return Ok(Some(format!("tmdb:{tmdb}")));
        }
        if let Some(tmdb) = ext.get("tmdb_id").and_then(|v| v.as_str()) {
            return Ok(Some(format!("tmdb:{tmdb}")));
        }
    }
    Ok(None)
}

/// Batch-match a slice of library items, returning a vec of
/// `(item, Option<cameo_media_id>)` tuples.
pub async fn match_library_items(
    items: &[ConnectorLibraryItem],
) -> Result<Vec<(ConnectorLibraryItem, Option<String>)>, StreamingError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let cameo_id = match_library_item(item).await?;
        out.push((item.clone(), cameo_id));
    }
    Ok(out)
}
