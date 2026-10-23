use salvo::prelude::*;
use stacks_core::blocklist::service::BlocklistService;
use stacks_core::media::catalog;

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;
use crate::media::{ResolveError, ensure_media, find_media};

use super::dto::*;
use super::error::*;

/// List all blocked media for the authenticated user.
#[salvo::oapi::endpoint(tags("Blocklist"), summary = "List blocklist")]
pub async fn list_blocklist(depot: &mut Depot) -> Result<Json<BlocklistResponse>, AddBlockError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = BlocklistService::new(state.db.clone());
    let items = svc
        .list_blocked(user.id)
        .await
        .map_err(|e| AddBlockError::Internal(crate::internal!(e)))?;
    let ids: Vec<_> = items.iter().map(|b| b.media_id).collect();
    let refs = catalog::primary_refs(&state.db, &ids)
        .await
        .map_err(|e| AddBlockError::Internal(crate::internal!(e)))?;
    Ok(Json(BlocklistResponse {
        items: items
            .into_iter()
            .filter_map(|b| {
                Some(BlocklistEntryResponse {
                    cameo_media_id: refs.get(&b.media_id)?.to_string(),
                    added_at: b.created_at.to_rfc3339(),
                })
            })
            .collect(),
    }))
}

/// Add a media item to the user's blocklist.
#[salvo::oapi::endpoint(tags("Blocklist"), summary = "Add to blocklist")]
pub async fn add_to_blocklist(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<BlocklistEntryResponse>, AddBlockError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: AddBlocklistRequest = req.parse_json().await.into_internal()?;
    let media = ensure_media(&state, &body.cameo_media_id)
        .await
        .map_err(|e| match e {
            ResolveError::NotFound => AddBlockError::MediaNotFound,
            ResolveError::Internal(e) => AddBlockError::Internal(crate::internal!(e)),
        })?;

    let svc = BlocklistService::new(state.db);
    let entry = svc
        .block(user.id, media.id)
        .await
        .map_err(|e| AddBlockError::Internal(crate::internal!(e)))?;
    Ok(Json(BlocklistEntryResponse {
        cameo_media_id: body.cameo_media_id,
        added_at: entry.created_at.to_rfc3339(),
    }))
}

/// Remove a media item from the user's blocklist.
#[salvo::oapi::endpoint(tags("Blocklist"), summary = "Remove from blocklist")]
pub async fn remove_from_blocklist(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, RemoveBlockError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media = find_media(&state, &cameo_id)
        .await
        .map_err(|e| RemoveBlockError::Internal(crate::internal!(e)))?
        .ok_or(RemoveBlockError::NotFound)?;
    let svc = BlocklistService::new(state.db);
    let is_blocked = svc
        .is_blocked(user.id, media.id)
        .await
        .map_err(|e| RemoveBlockError::Internal(crate::internal!(e)))?;
    if !is_blocked {
        return Err(RemoveBlockError::NotFound);
    }
    svc.unblock(user.id, media.id)
        .await
        .map_err(|e| RemoveBlockError::Internal(crate::internal!(e)))?;
    Ok(Json(()))
}
