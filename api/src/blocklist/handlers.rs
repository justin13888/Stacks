use salvo::prelude::*;
use stacks_core::blocklist::service::BlocklistService;

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

fn entry_to_dto(b: stacks_core::entity::blocklist::Model) -> BlocklistEntryResponse {
    BlocklistEntryResponse {
        cameo_media_id: b.cameo_media_id,
        added_at: b.added_at.to_rfc3339(),
    }
}

/// List all blocked media for the authenticated user.
#[salvo::oapi::endpoint(tags("Blocklist"), summary = "List blocklist")]
pub async fn list_blocklist(depot: &mut Depot) -> Result<Json<BlocklistResponse>, AddBlockError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = BlocklistService::new(state.db);
    let items = svc
        .list_blocked(&user.id)
        .await
        .map_err(|e| AddBlockError::Internal(crate::internal!(e)))?;
    Ok(Json(BlocklistResponse {
        items: items.into_iter().map(entry_to_dto).collect(),
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

    let svc = BlocklistService::new(state.db);
    let entry = svc
        .block(&user.id, &body.cameo_media_id)
        .await
        .map_err(|e| AddBlockError::Internal(crate::internal!(e)))?;
    Ok(Json(entry_to_dto(entry)))
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
    let svc = BlocklistService::new(state.db);
    // Check if entry exists first so we can return 404 if not.
    let is_blocked = svc
        .is_blocked(&user.id, &cameo_id)
        .await
        .map_err(|e| RemoveBlockError::Internal(crate::internal!(e)))?;
    if !is_blocked {
        return Err(RemoveBlockError::NotFound);
    }
    svc.unblock(&user.id, &cameo_id)
        .await
        .map_err(|e| RemoveBlockError::Internal(crate::internal!(e)))?;
    Ok(Json(()))
}
