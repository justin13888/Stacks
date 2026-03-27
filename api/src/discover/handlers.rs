use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Get trending media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Trending media")]
pub async fn trending(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    todo!()
}

/// Get popular media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Popular media")]
pub async fn popular(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    todo!()
}

/// Get upcoming releases.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Upcoming media")]
pub async fn upcoming(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    todo!()
}

/// Get top-rated media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Top-rated media")]
pub async fn top_rated(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    todo!()
}
