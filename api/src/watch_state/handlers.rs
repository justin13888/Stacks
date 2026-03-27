use salvo::prelude::*;
use stacks_core::watch_state::{WatchStateError, WatchStateService};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

fn status_to_dto(m: stacks_core::entity::watch_status::Model) -> WatchStatusResponse {
    WatchStatusResponse {
        cameo_media_id: m.cameo_media_id,
        status: m.status,
        score: m.score,
        notes: m.notes,
        is_favorite: m.is_favorite,
        started_at: m.started_at.map(|dt| dt.to_rfc3339()),
        completed_at: m.completed_at.map(|dt| dt.to_rfc3339()),
        updated_at: m.updated_at.to_rfc3339(),
    }
}

fn episode_to_dto(m: stacks_core::entity::episode_watch_status::Model) -> EpisodeStatusResponse {
    EpisodeStatusResponse {
        cameo_episode_id: m.cameo_episode_id,
        watched: m.watched,
        progress_seconds: m.progress_seconds,
        updated_at: m.updated_at.to_rfc3339(),
    }
}

/// Get watch status for a media item.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Get media watch status")]
pub async fn get_media_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<WatchStatusResponse>, GetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = WatchStateService::new(state.db);
    let status = svc
        .get_status(&user.id, &cameo_id)
        .await
        .map_err(|e| match e {
            WatchStateError::NotFound => GetStatusError::NotFound,
            e => GetStatusError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(status_to_dto(status)))
}

/// Create or update watch status for a media item.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Set media watch status")]
pub async fn set_media_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<WatchStatusResponse>, SetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: WatchStatusRequest = req.parse_json().await.into_internal()?;

    let svc = WatchStateService::new(state.db);
    let status = svc
        .set_status(
            &user.id,
            &cameo_id,
            body.status,
            body.score,
            body.notes,
            body.is_favorite,
        )
        .await
        .map_err(|e| match e {
            WatchStateError::InvalidStatus(_) => SetStatusError::InvalidStatus,
            WatchStateError::InvalidScore => SetStatusError::InvalidScore,
            e => SetStatusError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(status_to_dto(status)))
}

/// Delete watch status for a media item.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Delete media watch status")]
pub async fn delete_media_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = WatchStateService::new(state.db);
    svc.delete_status(&user.id, &cameo_id)
        .await
        .map_err(|e| match e {
            WatchStateError::NotFound => DeleteStatusError::NotFound,
            e => DeleteStatusError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(()))
}

/// Get watch status for an episode.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Get episode watch status")]
pub async fn get_episode_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<EpisodeStatusResponse>, GetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let episode_id = req.param::<String>("episode_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = WatchStateService::new(state.db);
    let status = svc
        .get_episode_status(&user.id, &episode_id)
        .await
        .map_err(|e| match e {
            WatchStateError::NotFound => GetStatusError::NotFound,
            e => GetStatusError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(episode_to_dto(status)))
}

/// Create or update watch status for an episode.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Set episode watch status")]
pub async fn set_episode_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<EpisodeStatusResponse>, SetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let episode_id = req.param::<String>("episode_id").unwrap_or_default();
    // The media_id can be supplied as a query param or we leave it empty for now.
    let cameo_media_id = req.query::<String>("cameo_media_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: EpisodeStatusRequest = req.parse_json().await.into_internal()?;

    let svc = WatchStateService::new(state.db);
    let status = svc
        .set_episode_status(
            &user.id,
            &episode_id,
            &cameo_media_id,
            body.watched,
            body.progress_seconds,
        )
        .await
        .map_err(|e| SetStatusError::Internal(crate::internal!(e)))?;
    Ok(Json(episode_to_dto(status)))
}

/// Bulk-update watch status for multiple media items.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Bulk update watch status")]
pub async fn bulk_update_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<BulkStatusResponse>, BulkUpdateError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: BulkStatusRequest = req.parse_json().await.into_internal()?;

    let entries: Vec<stacks_core::watch_state::service::BulkEntry> = body
        .entries
        .into_iter()
        .map(|e| stacks_core::watch_state::service::BulkEntry {
            cameo_media_id: e.cameo_media_id,
            status: e.status,
            score: e.score,
            notes: None,
            is_favorite: None,
        })
        .collect();

    let svc = WatchStateService::new(state.db);
    let updated = svc
        .bulk_update(&user.id, entries)
        .await
        .map_err(|e| match e {
            WatchStateError::InvalidStatus(_) => BulkUpdateError::InvalidStatus,
            WatchStateError::InvalidScore => BulkUpdateError::InvalidScore,
            e => BulkUpdateError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(BulkStatusResponse {
        updated: updated.len() as u32,
    }))
}
