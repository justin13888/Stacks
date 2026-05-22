//! Overlays the current user's watch state and streaming availability onto the
//! base media items produced by [`mapping`](super::mapping).

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::{server_library_item, streaming_server, watch_status};

use super::MediaError;
use super::id::MediaId;
use super::models::{AvailabilityInfo, MediaWithUserState, ServerAvailability};

/// Overlay user watch state and streaming availability onto a batch of base
/// media items.
///
/// Issues exactly two database queries regardless of the batch size: one for
/// the user's watch status and one for streaming availability.
pub async fn enrich(
    db: &DatabaseConnection,
    user_id: &str,
    mut items: Vec<MediaWithUserState>,
) -> Result<Vec<MediaWithUserState>, MediaError> {
    if items.is_empty() {
        return Ok(items);
    }
    let ids: Vec<String> = items.iter().map(|m| m.cameo_media_id.clone()).collect();

    // One query for the user's watch status across every id in the batch.
    let statuses: HashMap<String, watch_status::Model> = watch_status::Entity::find()
        .filter(watch_status::Column::UserId.eq(user_id))
        .filter(watch_status::Column::CameoMediaId.is_in(ids.clone()))
        .all(db)
        .await?
        .into_iter()
        .map(|m| (m.cameo_media_id.clone(), m))
        .collect();

    // One query for streaming availability across the batch.
    let availability = availability_map(db, &ids).await?;

    for item in &mut items {
        if let Some(status) = statuses.get(&item.cameo_media_id) {
            item.watch_status = Some(status.status.clone());
            item.score = status.score;
            item.is_favorite = status.is_favorite;
        }
        item.availability = availability
            .get(&item.cameo_media_id)
            .cloned()
            .unwrap_or_default();
    }
    Ok(items)
}

/// Overlay user state onto a single base media item.
pub async fn enrich_one(
    db: &DatabaseConnection,
    user_id: &str,
    item: MediaWithUserState,
) -> Result<MediaWithUserState, MediaError> {
    let mut enriched = enrich(db, user_id, vec![item]).await?;
    // `enrich` preserves length, so exactly one item is returned.
    Ok(enriched.remove(0))
}

/// Build the [`AvailabilityInfo`] for a single media id.
pub async fn availability_for(
    db: &DatabaseConnection,
    media_id: &MediaId,
) -> Result<AvailabilityInfo, MediaError> {
    let id = media_id.to_storage_string();
    Ok(availability_map(db, std::slice::from_ref(&id))
        .await?
        .remove(&id)
        .unwrap_or_default())
}

/// Query `server_library_items` (joined to their owning servers) for a set of
/// cameo media ids, grouped into per-id [`AvailabilityInfo`].
async fn availability_map(
    db: &DatabaseConnection,
    ids: &[String],
) -> Result<HashMap<String, AvailabilityInfo>, MediaError> {
    let rows = server_library_item::Entity::find()
        .filter(server_library_item::Column::CameoMediaId.is_in(ids.to_vec()))
        .find_also_related(streaming_server::Entity)
        .all(db)
        .await?;

    let mut map: HashMap<String, AvailabilityInfo> = HashMap::new();
    for (item, server) in rows {
        let Some(cameo_media_id) = item.cameo_media_id.clone() else {
            continue;
        };
        let server_name = server
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Unknown server".to_owned());
        let entry = map.entry(cameo_media_id).or_default();
        entry.available = true;
        entry.servers.push(ServerAvailability {
            server_id: item.server_id,
            server_name,
            quality: item.quality,
            deep_link: item.deep_link,
            playback_link: item.playback_link,
        });
    }
    Ok(map)
}
