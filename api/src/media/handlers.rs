use salvo::prelude::*;

use stacks_core::media::filter::FilterQuery;
use stacks_core::media::{MediaError, MediaService};

use crate::app::AppState;
use crate::extractors::auth_user::require_user;
use crate::extractors::pagination;

use super::dto::*;
use super::error::*;

/// Search for media by query string with optional pagination.
#[salvo::oapi::endpoint(tags("Media"), summary = "Search media")]
pub async fn search_media(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<MediaSearchResponse>, SearchError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let q = req.query::<String>("q").unwrap_or_default();
    let pag = pagination::from_query(req);
    let svc = MediaService::new(state.db, state.cameo);
    let result = svc
        .search(&user.id, &q, &FilterQuery::default(), &pag)
        .await
        .map_err(|e| SearchError::Internal(crate::internal!(e)))?;
    Ok(Json(MediaSearchResponse {
        items: result
            .items
            .into_iter()
            .map(|i| serde_json::to_value(i).unwrap_or(serde_json::Value::Null))
            .collect(),
        next_cursor: result.next_cursor,
    }))
}

/// Get full details for a single media item by Cameo ID.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get media detail")]
pub async fn get_media(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<MediaDetailResponse>, GetMediaError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let svc = MediaService::new(state.db, state.cameo);
    let item = svc.get_detail(&user.id, &cameo_id).await.map_err(|e| match e {
        MediaError::NotFound(_) => GetMediaError::NotFound,
        e => GetMediaError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(MediaDetailResponse(
        serde_json::to_value(item).unwrap_or(serde_json::Value::Null),
    )))
}

/// Get season list for a TV show.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get seasons")]
pub async fn get_seasons(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<SeasonsResponse>, GetSeasonsError> {
    let _user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let svc = MediaService::new(state.db, state.cameo);
    let seasons = svc.get_seasons(&cameo_id).await.map_err(|e| match e {
        MediaError::NotFound(_) => GetSeasonsError::NotFound,
        e => GetSeasonsError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(SeasonsResponse { seasons }))
}

/// Get episode list for a specific season.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get episodes")]
pub async fn get_episodes(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<EpisodesResponse>, GetEpisodesError> {
    let _user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let season = req.param::<u32>("season").unwrap_or(1);
    let svc = MediaService::new(state.db, state.cameo);
    let episodes = svc
        .get_episodes(&cameo_id, season)
        .await
        .map_err(|e| match e {
            MediaError::NotFound(_) => GetEpisodesError::NotFound,
            e => GetEpisodesError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(EpisodesResponse { episodes }))
}

/// Get recommendations based on a media item.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get recommendations")]
pub async fn get_recommendations(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<MediaSearchResponse>, GetMediaError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let pag = pagination::from_query(req);
    let svc = MediaService::new(state.db, state.cameo);
    let result = svc
        .get_recommendations(&user.id, &cameo_id, &pag)
        .await
        .map_err(|e| match e {
            MediaError::NotFound(_) => GetMediaError::NotFound,
            e => GetMediaError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(MediaSearchResponse {
        items: result
            .items
            .into_iter()
            .map(|i| serde_json::to_value(i).unwrap_or(serde_json::Value::Null))
            .collect(),
        next_cursor: result.next_cursor,
    }))
}

/// Resolve hydrated metadata for a batch of cameo IDs.
#[salvo::oapi::endpoint(tags("Media"), summary = "Batch get media")]
pub async fn batch_get(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<MediaSearchResponse>, SearchError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: super::dto::MediaBatchRequest = req
        .parse_json()
        .await
        .map_err(|e| SearchError::Internal(crate::internal!(e)))?;
    let svc = MediaService::new(state.db, state.cameo);
    let items = svc
        .batch_get(&user.id, &body.cameo_ids)
        .await
        .map_err(|e| SearchError::Internal(crate::internal!(e)))?;
    Ok(Json(MediaSearchResponse {
        items: items
            .into_iter()
            .map(|i| serde_json::to_value(i).unwrap_or(serde_json::Value::Null))
            .collect(),
        next_cursor: None,
    }))
}
