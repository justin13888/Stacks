//! Logic to merge cameo metadata with user watch state and streaming availability.

use sea_orm::DatabaseConnection;

use super::models::{AvailabilityInfo, MediaWithUserState};

/// Enrich a list of cameo media IDs with the user's watch state and availability.
pub async fn enrich_many(
    _db: &DatabaseConnection,
    _user_id: &str,
    _cameo_ids: &[String],
) -> Vec<MediaWithUserState> {
    // TODO: batch query watch_status and server_library_items,
    // then zip with cameo metadata.
    todo!("enrich_many")
}

/// Enrich a single cameo media item.
pub async fn enrich_one(
    _db: &DatabaseConnection,
    _user_id: &str,
    _cameo_id: &str,
) -> Option<MediaWithUserState> {
    todo!("enrich_one")
}

/// Build availability info for a cameo_media_id.
pub async fn get_availability(_db: &DatabaseConnection, _cameo_id: &str) -> AvailabilityInfo {
    todo!("get_availability")
}
