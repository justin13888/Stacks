//! Overlays the current user's library state onto base media items produced
//! by [`mapping`](super::mapping) or [`MediaWithUserState::from_snapshot`].

use std::collections::HashMap;

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{entry_progress, library_entry};

use super::MediaError;
use super::catalog;
use super::id::MediaId;
use super::models::MediaWithUserState;

/// Overlay library state onto a batch of base media items.
///
/// Issues a fixed number of queries regardless of batch size: one to resolve
/// public ids to catalog rows, one for the user's entries, one for progress.
/// Titles the catalog has never seen are simply untracked.
pub async fn enrich<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    mut items: Vec<MediaWithUserState>,
) -> Result<Vec<MediaWithUserState>, MediaError> {
    if items.is_empty() {
        return Ok(items);
    }
    let ids: Vec<MediaId> = items
        .iter()
        .filter_map(|m| MediaId::parse(&m.cameo_media_id).ok())
        .collect();
    let resolved = catalog::resolve_refs(db, &ids).await?;
    if resolved.is_empty() {
        return Ok(items);
    }

    let entries: HashMap<Uuid, library_entry::Model> = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .filter(library_entry::Column::DeletedAt.is_null())
        .filter(library_entry::Column::MediaId.is_in(resolved.values().copied()))
        .all(db)
        .await?
        .into_iter()
        .map(|e| (e.media_id, e))
        .collect();
    let progress: HashMap<Uuid, entry_progress::Model> = entry_progress::Entity::find()
        .filter(entry_progress::Column::EntryId.is_in(entries.values().map(|e| e.id)))
        .all(db)
        .await?
        .into_iter()
        .map(|p| (p.entry_id, p))
        .collect();

    for item in &mut items {
        let Some(entry) = MediaId::parse(&item.cameo_media_id)
            .ok()
            .and_then(|id| resolved.get(&id))
            .and_then(|media_id| entries.get(media_id))
        else {
            continue;
        };
        apply_entry(item, entry, progress.get(&entry.id));
    }
    Ok(items)
}

/// Copy one entry's state onto an item.
pub fn apply_entry(
    item: &mut MediaWithUserState,
    entry: &library_entry::Model,
    progress: Option<&entry_progress::Model>,
) {
    item.watch_status = Some(entry.status);
    item.score = entry.score.map(i32::from);
    item.is_favorite = entry.favourite;
    item.rewatching = progress.is_some_and(|p| p.run_count > 1);
}

/// Overlay library state onto a single base media item.
pub async fn enrich_one<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    item: MediaWithUserState,
) -> Result<MediaWithUserState, MediaError> {
    let mut enriched = enrich(db, user_id, vec![item]).await?;
    // `enrich` preserves length, so exactly one item is returned.
    Ok(enriched.remove(0))
}
