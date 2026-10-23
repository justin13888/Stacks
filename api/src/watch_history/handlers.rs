//! The history feed, served from the append-only `entry_events` log.

use salvo::prelude::*;
use stacks_core::library::{LibraryService, LogPlay};
use stacks_core::media::{EpisodeId, MediaId};
use stacks_core::pagination::CursorParams;

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;
use crate::media::{ResolveError, ensure_media, find_media};

use super::dto::*;
use super::error::*;

/// List the authenticated user's history (every library change), newest
/// first.
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
    let media_id = match req.query::<String>("cameo_media_id") {
        Some(raw) => match find_media(&state, &raw)
            .await
            .map_err(|e| ListHistoryError::Internal(crate::internal!(e)))?
        {
            Some(m) => Some(m.id),
            // A title never stored has no history.
            None => {
                return Ok(Json(HistoryListResponse {
                    items: Vec::new(),
                    next_cursor: None,
                }));
            }
        },
        None => None,
    };

    let result = LibraryService::new(state.db)
        .history(user.id, media_id, &cursor)
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

/// Record that something was watched. Only `complete` is accepted: it logs a
/// play (of the episode, when one is given). Playback-progress events have no
/// meaning without a streaming server and are rejected.
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

    if body.event != "complete" {
        return Err(AddHistoryError::InvalidEvent);
    }
    MediaId::parse(&body.cameo_media_id).map_err(|_| AddHistoryError::InvalidEvent)?;
    let episode = match body.cameo_episode_id.as_deref() {
        Some(raw) => {
            let id = EpisodeId::parse(raw).map_err(|_| AddHistoryError::InvalidEvent)?;
            Some((
                i32::try_from(id.season).map_err(|_| AddHistoryError::InvalidEvent)?,
                i32::try_from(id.episode).map_err(|_| AddHistoryError::InvalidEvent)?,
            ))
        }
        None => None,
    };
    let watched_at = match body.timestamp.as_deref() {
        Some(ts) => Some(
            chrono::DateTime::parse_from_rfc3339(ts).map_err(|_| AddHistoryError::InvalidEvent)?,
        ),
        None => Some(stacks_core::db::now()),
    };
    let media = ensure_media(&state, &body.cameo_media_id)
        .await
        .map_err(|e| match e {
            ResolveError::NotFound => AddHistoryError::InvalidEvent,
            ResolveError::Internal(e) => AddHistoryError::Internal(crate::internal!(e)),
        })?;
    let mut log = LogPlay::manual(episode);
    log.watched_at = watched_at;
    let play = LibraryService::new(state.db)
        .log_play(user.id, &media, log)
        .await
        .map_err(|e| match e {
            stacks_core::library::LibraryError::NotAShow => AddHistoryError::InvalidEvent,
            e => AddHistoryError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(
        serde_json::to_value(&play).unwrap_or(serde_json::Value::Null),
    ))
}
