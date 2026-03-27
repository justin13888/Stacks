use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// Get watch statistics for the authenticated user.
#[salvo::oapi::endpoint(tags("Stats"), summary = "Get user stats")]
pub async fn get_stats(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<StatsResponse>, GetStatsError> {
    todo!()
}
