use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// List the authenticated user's watch history.
#[salvo::oapi::endpoint(tags("Watch History"), summary = "List watch history")]
pub async fn list_history(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<HistoryListResponse>, ListHistoryError> {
    todo!()
}

/// Manually add a history entry.
#[salvo::oapi::endpoint(tags("Watch History"), summary = "Add history entry")]
pub async fn add_history(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<serde_json::Value>, AddHistoryError> {
    todo!()
}
