use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Search for media by query string with optional pagination.
#[salvo::oapi::endpoint(tags("Media"), summary = "Search media")]
pub async fn search_media(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<MediaSearchResponse>, SearchError> {
    todo!()
}

/// Get full details for a single media item by Cameo ID.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get media detail")]
pub async fn get_media(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<MediaDetailResponse>, GetMediaError> {
    todo!()
}

/// Get season list for a TV show.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get seasons")]
pub async fn get_seasons(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<SeasonsResponse>, GetSeasonsError> {
    todo!()
}

/// Get episode list for a specific season.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get episodes")]
pub async fn get_episodes(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<EpisodesResponse>, GetEpisodesError> {
    todo!()
}

/// Get recommendations based on a media item.
#[salvo::oapi::endpoint(tags("Media"), summary = "Get recommendations")]
pub async fn get_recommendations(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<MediaSearchResponse>, GetMediaError> {
    todo!()
}
