use crate::media::id::{MediaId, MediaKind, Provider};

use super::models::ConnectorLibraryItem;

/// Attempt to match a connector library item to a canonical Stacks media id
/// (e.g. `tmdb:movie:550`) using its external provider ids.
///
/// Matching is conservative: it succeeds only when the item carries both a
/// recognised provider id (currently TMDB) and a media-type hint that
/// disambiguates movie from TV. Ambiguous items return `None` rather than a
/// guessed id, so a wrong match never enters the library.
pub fn match_library_item(item: &ConnectorLibraryItem) -> Option<String> {
    let external = item.external_ids.as_ref()?;

    let kind = match external
        .get("media_type")
        .or_else(|| external.get("type"))
        .and_then(serde_json::Value::as_str)
    {
        Some("tv" | "show" | "series") => MediaKind::Tv,
        Some("movie" | "film") => MediaKind::Movie,
        _ => return None,
    };

    let tmdb_id = external
        .get("tmdb")
        .or_else(|| external.get("tmdb_id"))
        .and_then(extract_id)?;

    Some(MediaId::new(Provider::Tmdb, kind, tmdb_id).to_storage_string())
}

/// Match a batch of items, pairing each with its resolved media id (if any).
pub fn match_library_items(
    items: Vec<ConnectorLibraryItem>,
) -> Vec<(ConnectorLibraryItem, Option<String>)> {
    items
        .into_iter()
        .map(|item| {
            let matched = match_library_item(&item);
            (item, matched)
        })
        .collect()
}

/// Pull an integer id out of a JSON value that may be a number or a string.
fn extract_id(value: &serde_json::Value) -> Option<i32> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .and_then(|n| i32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(external_ids: serde_json::Value) -> ConnectorLibraryItem {
        ConnectorLibraryItem {
            server_internal_id: "x".to_string(),
            title: "t".to_string(),
            quality: None,
            deep_link: "d".to_string(),
            playback_link: "p".to_string(),
            external_ids: Some(external_ids),
        }
    }

    #[test]
    fn matches_tmdb_movie() {
        let m = match_library_item(&item(
            serde_json::json!({"tmdb": 550, "media_type": "movie"}),
        ));
        assert_eq!(m.as_deref(), Some("tmdb:movie:550"));
    }

    #[test]
    fn rejects_ambiguous_or_unknown() {
        // No media-type hint — cannot tell movie from TV.
        assert!(match_library_item(&item(serde_json::json!({"tmdb": 550}))).is_none());
        // No recognised provider id.
        assert!(match_library_item(&item(serde_json::json!({"media_type": "movie"}))).is_none());
    }
}
