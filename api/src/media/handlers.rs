use salvo::prelude::*;
use stacks_core::media::{MediaError, MediaId, MediaService};

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
    let query = req.query::<String>("q").unwrap_or_default();
    let pg = pagination::from_query(req);

    let svc = MediaService::new(state.db, state.cameo);
    let result = svc
        .search(&user.id, &query, &pg)
        .await
        .map_err(|e| SearchError::Internal(crate::internal!(e)))?;
    Ok(Json(MediaSearchResponse {
        items: result.items.into_iter().map(MediaItem::from).collect(),
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
    let media_id = parse_media_id(req).ok_or(GetMediaError::NotFound)?;

    let svc = MediaService::new(state.db, state.cameo);
    let detail = svc
        .get_detail(&user.id, &media_id)
        .await
        .map_err(to_get_media_error)?;
    Ok(Json(detail.into()))
}

/// Get the season list for a TV show.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get seasons")]
pub async fn get_seasons(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<SeasonsResponse>, GetSeasonsError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media_id = parse_media_id(req).ok_or(GetSeasonsError::NotFound)?;

    let svc = MediaService::new(state.db, state.cameo);
    let seasons = svc.get_seasons(&media_id).await.map_err(|e| match e {
        MediaError::NotFound(_) => GetSeasonsError::NotFound,
        e => GetSeasonsError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(SeasonsResponse {
        seasons: seasons.into_iter().map(Season::from).collect(),
    }))
}

/// Get the episode list for a specific season.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get episodes")]
pub async fn get_episodes(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<EpisodesResponse>, GetEpisodesError> {
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media_id = parse_media_id(req).ok_or(GetEpisodesError::NotFound)?;
    let season = req.param::<u32>("season").unwrap_or_default();

    let svc = MediaService::new(state.db, state.cameo);
    let episodes = svc
        .get_episodes(&media_id, season)
        .await
        .map_err(|e| match e {
            MediaError::NotFound(_) => GetEpisodesError::NotFound,
            e => GetEpisodesError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(EpisodesResponse {
        episodes: episodes.into_iter().map(Episode::from).collect(),
    }))
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
    let media_id = parse_media_id(req).ok_or(GetMediaError::NotFound)?;
    let pg = pagination::from_query(req);

    let svc = MediaService::new(state.db, state.cameo);
    let result = svc
        .get_recommendations(&user.id, &media_id, &pg)
        .await
        .map_err(to_get_media_error)?;
    Ok(Json(MediaSearchResponse {
        items: result.items.into_iter().map(MediaItem::from).collect(),
        next_cursor: result.next_cursor,
    }))
}

/// Parse the `cameo_id` path parameter into a [`MediaId`].
fn parse_media_id(req: &Request) -> Option<MediaId> {
    let raw = req.param::<String>("cameo_id")?;
    MediaId::parse(&raw).ok()
}

/// Map a core [`MediaError`] onto the media-detail API error.
fn to_get_media_error(e: MediaError) -> GetMediaError {
    match e {
        MediaError::NotFound(_) => GetMediaError::NotFound,
        e => GetMediaError::Internal(crate::internal!(e)),
    }
}
