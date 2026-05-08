use salvo::prelude::*;

use stacks_core::media::MediaService;

use crate::app::AppState;
use crate::extractors::auth_user::require_user;
use crate::extractors::pagination;

use super::dto::*;
use super::error::*;

async fn discover(
    category: &str,
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media_type = req.query::<String>("media_type");
    let pag = pagination::from_query(req);
    let svc = MediaService::new(state.db, state.cameo);
    let result = svc
        .discover(&user.id, category, media_type.as_deref(), &pag)
        .await
        .map_err(|e| DiscoverError::Internal(crate::internal!(e)))?;
    Ok(Json(DiscoverResponse {
        items: result
            .items
            .into_iter()
            .map(|i| serde_json::to_value(i).unwrap_or(serde_json::Value::Null))
            .collect(),
        next_cursor: result.next_cursor,
    }))
}

/// Get trending media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Trending media")]
pub async fn trending(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover("trending", req, depot).await
}

/// Get popular media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Popular media")]
pub async fn popular(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover("popular", req, depot).await
}

/// Get upcoming releases.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Upcoming media")]
pub async fn upcoming(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover("upcoming", req, depot).await
}

/// Get top-rated media.
#[salvo::oapi::endpoint(tags("Discover"), summary = "Top-rated media")]
pub async fn top_rated(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<DiscoverResponse>, DiscoverError> {
    discover("top_rated", req, depot).await
}
