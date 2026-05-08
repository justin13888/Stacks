//! Logic to merge cameo metadata with user watch state and streaming availability.

use std::collections::HashMap;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::{server_library_item, streaming_server, watch_status};

use super::models::{AvailabilityInfo, MediaWithUserState, ServerAvailability};

/// Enrich a list of cameo media IDs with the user's watch state and availability.
///
/// Returns one stub `MediaWithUserState` per id with state fields filled in
/// (title/genres/etc. left empty — caller is expected to overlay these from
/// the cameo response). The returned vec is in the same order as `cameo_ids`.
pub async fn enrich_many(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_ids: &[String],
) -> Vec<MediaWithUserState> {
    if cameo_ids.is_empty() {
        return Vec::new();
    }

    let watch_statuses: HashMap<String, watch_status::Model> = watch_status::Entity::find()
        .filter(watch_status::Column::UserId.eq(user_id))
        .filter(watch_status::Column::CameoMediaId.is_in(cameo_ids.iter().cloned()))
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| (m.cameo_media_id.clone(), m))
        .collect();

    let library_items: Vec<server_library_item::Model> = server_library_item::Entity::find()
        .filter(server_library_item::Column::CameoMediaId.is_in(cameo_ids.iter().cloned()))
        .all(db)
        .await
        .unwrap_or_default();

    let server_ids: Vec<String> = library_items
        .iter()
        .map(|i| i.server_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let servers: HashMap<String, streaming_server::Model> = if server_ids.is_empty() {
        HashMap::new()
    } else {
        streaming_server::Entity::find()
            .filter(streaming_server::Column::Id.is_in(server_ids))
            .all(db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };

    let mut by_cameo: HashMap<String, Vec<ServerAvailability>> = HashMap::new();
    for item in library_items {
        let Some(cameo_id) = item.cameo_media_id.clone() else {
            continue;
        };
        let server = match servers.get(&item.server_id) {
            Some(s) => s,
            None => continue,
        };
        by_cameo.entry(cameo_id).or_default().push(ServerAvailability {
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            quality: item.quality,
            deep_link: item.deep_link,
            playback_link: item.playback_link,
        });
    }

    cameo_ids
        .iter()
        .map(|id| {
            let ws = watch_statuses.get(id);
            let avail = by_cameo.get(id).cloned().unwrap_or_default();
            MediaWithUserState {
                cameo_media_id: id.clone(),
                title: String::new(),
                media_type: String::new(),
                year: None,
                overview: None,
                poster_url: None,
                backdrop_url: None,
                genres: Vec::new(),
                rating: None,
                watch_status: ws.map(|m| m.status.clone()),
                score: ws.and_then(|m| m.score),
                is_favorite: ws.map(|m| m.is_favorite).unwrap_or(false),
                availability: AvailabilityInfo {
                    available: !avail.is_empty(),
                    servers: avail,
                },
            }
        })
        .collect()
}

/// Enrich a single cameo media item.
pub async fn enrich_one(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_id: &str,
) -> Option<MediaWithUserState> {
    let mut many = enrich_many(db, user_id, &[cameo_id.to_string()]).await;
    many.pop()
}

/// Build availability info for a cameo_media_id (no user-specific data).
pub async fn get_availability(db: &DatabaseConnection, cameo_id: &str) -> AvailabilityInfo {
    let library_items: Vec<server_library_item::Model> = server_library_item::Entity::find()
        .filter(server_library_item::Column::CameoMediaId.eq(cameo_id))
        .all(db)
        .await
        .unwrap_or_default();

    if library_items.is_empty() {
        return AvailabilityInfo::default();
    }

    let server_ids: Vec<String> = library_items
        .iter()
        .map(|i| i.server_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let servers: HashMap<String, streaming_server::Model> = streaming_server::Entity::find()
        .filter(streaming_server::Column::Id.is_in(server_ids))
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();

    let availability: Vec<ServerAvailability> = library_items
        .into_iter()
        .filter_map(|item| {
            let server = servers.get(&item.server_id)?;
            Some(ServerAvailability {
                server_id: server.id.clone(),
                server_name: server.name.clone(),
                quality: item.quality,
                deep_link: item.deep_link,
                playback_link: item.playback_link,
            })
        })
        .collect();

    AvailabilityInfo {
        available: !availability.is_empty(),
        servers: availability,
    }
}
