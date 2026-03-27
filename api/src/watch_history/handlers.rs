use salvo::prelude::*;
use stacks_core::pagination::CursorParams;
use stacks_core::watch_history::service::{RecordEventParams, WatchHistoryService};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

const VALID_EVENTS: &[&str] = &["play", "pause", "stop", "complete", "seek", "progress"];

/// List the authenticated user's watch history.
#[salvo::oapi::endpoint(tags("Watch History"), summary = "List watch history")]
pub async fn list_history(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<HistoryListResponse>, ListHistoryError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();

    let cursor = CursorParams {
        cursor: req.query::<String>("cursor"),
        limit: req.query::<u64>("limit"),
    };
    let cameo_media_id = req.query::<String>("cameo_media_id");

    let svc = WatchHistoryService::new(state.db);
    let result = svc
        .list_history(&user.id, cursor, cameo_media_id.as_deref())
        .await
        .map_err(|e| ListHistoryError::Internal(crate::internal!(e)))?;

    let items: Vec<serde_json::Value> = result
        .items
        .into_iter()
        .map(|m| serde_json::to_value(&m).unwrap_or(serde_json::Value::Null))
        .collect();

    Ok(Json(HistoryListResponse {
        items,
        next_cursor: result.next_cursor,
    }))
}

/// Manually add a history entry.
#[salvo::oapi::endpoint(tags("Watch History"), summary = "Add history entry")]
pub async fn add_history(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<serde_json::Value>, AddHistoryError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: ManualHistoryRequest = req.parse_json().await.into_internal()?;

    if !VALID_EVENTS.contains(&body.event.as_str()) {
        return Err(AddHistoryError::InvalidEvent);
    }

    let svc = WatchHistoryService::new(state.db);
    let params = RecordEventParams {
        user_id: &user.id,
        cameo_media_id: &body.cameo_media_id,
        cameo_episode_id: body.cameo_episode_id.as_deref(),
        event: body.event,
        position_seconds: body.position_seconds,
        metadata: None,
        source: "manual".to_string(),
    };
    let record = svc
        .record_event(params)
        .await
        .map_err(|e| AddHistoryError::Internal(crate::internal!(e)))?;

    Ok(Json(
        serde_json::to_value(&record).unwrap_or(serde_json::Value::Null),
    ))
}
