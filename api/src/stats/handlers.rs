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
    let svc = StatsService::new(state.db, state.cameo);
    let stats = svc
        .get_user_stats(&user.id)
        .await
        .map_err(|e| GetStatsError::Internal(crate::internal!(e)))?;

    Ok(Json(StatsResponse {
        total_watch_time_minutes: stats.total_watch_time_minutes,
        genre_breakdown: stats
            .genre_breakdown
            .into_iter()
            .map(|(name, count)| serde_json::json!({ "genre": name, "count": count }))
            .collect(),
        yearly_counts: stats
            .yearly_counts
            .into_iter()
            .map(|(year, count)| serde_json::json!({ "year": year, "count": count }))
            .collect(),
        monthly_heatmap: stats
            .monthly_heatmap
            .into_iter()
            .map(|(month, count)| serde_json::json!({ "month": month, "count": count }))
            .collect(),
    }))
}
