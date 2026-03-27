use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Get watch status for a media item.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Get media watch status")]
pub async fn get_media_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<WatchStatusResponse>, GetStatusError> {
    todo!()
}

/// Create or update watch status for a media item.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Set media watch status")]
pub async fn set_media_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<WatchStatusResponse>, SetStatusError> {
    todo!()
}

/// Delete watch status for a media item.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Delete media watch status")]
pub async fn delete_media_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, DeleteStatusError> {
    todo!()
}

/// Get watch status for an episode.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Get episode watch status")]
pub async fn get_episode_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<EpisodeStatusResponse>, GetStatusError> {
    todo!()
}

/// Create or update watch status for an episode.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Set episode watch status")]
pub async fn set_episode_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<EpisodeStatusResponse>, SetStatusError> {
    todo!()
}

/// Bulk-update watch status for multiple media items.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Bulk update watch status")]
pub async fn bulk_update_status(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<BulkStatusResponse>, BulkUpdateError> {
    todo!()
}
