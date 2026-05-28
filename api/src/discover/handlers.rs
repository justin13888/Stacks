use salvo::prelude::*;
use stacks_core::media::{DiscoverCategory, MediaKind, MediaService};

use crate::app::AppState;
use crate::extractors::auth_user::require_user;
use crate::extractors::pagination;
use crate::media::dto::MediaItem;

use super::dto::*;
use super::error::*;

/// Shared implementation for every discovery feed.
async fn discover_feed(
    req: &mut Request,
    depot: &mut Depot,
    category: DiscoverCategory,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media_type = req
        .query::<String>("media_type")
        .and_then(|s| s.parse::<MediaKind>().ok());
    let pg = pagination::from_query(req);

    let svc = MediaService::new(state.db, state.cameo);
    let result = svc
        .discover(&user.id, category, media_type, &pg)
        .await
        .map_err(|e| DiscoverError::Internal(crate::internal!(e)))?;
    Ok(Json(DiscoverResponse {
        items: result.items.into_iter().map(MediaItem::from).collect(),
        next_cursor: result.next_cursor,
    }))
}

/// Get trending media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Trending media")]
pub async fn trending(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover_feed(req, depot, DiscoverCategory::Trending).await
}

/// Get popular media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Popular media")]
pub async fn popular(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover_feed(req, depot, DiscoverCategory::Popular).await
}

/// Get upcoming releases.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Upcoming media")]
pub async fn upcoming(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover_feed(req, depot, DiscoverCategory::Upcoming).await
}

/// Get top-rated media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Top-rated media")]
pub async fn top_rated(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover_feed(req, depot, DiscoverCategory::TopRated).await
}
