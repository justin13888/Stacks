use salvo::prelude::*;
use stacks_core::stats::StatsService;

use crate::app::AppState;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

/// Get watch statistics for the authenticated user.
#[salvo::oapi::endpoint(tags("Stats"), summary = "Get user stats")]
pub async fn get_stats(depot: &mut Depot) -> Result<Json<StatsResponse>, GetStatsError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = StatsService::new(state.db);
    let stats = svc
        .get_user_stats(&state.cameo, &user.id)
        .await
        .map_err(|e| GetStatsError::Internal(crate::internal!(e)))?;
    Ok(Json(stats.into()))
}
