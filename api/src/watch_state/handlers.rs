//! Library endpoints, adapted onto the library service. The request and
//! response shapes are unchanged so the current web client keeps working;
//! the API redesign replaces them.

use salvo::prelude::*;
use stacks_core::Patch;
use stacks_core::entity::enums::{DatePrecision, EntryStatus};
use stacks_core::entity::{library_entry, media};
use stacks_core::library::{
    EntryPatch, EntryView, LibraryError, LibraryFilter, LibraryService, LogPlay, RunDate, RunPatch,
};
use stacks_core::media::catalog;
use stacks_core::media::enrichment::apply_entry;
use stacks_core::media::models::MediaWithUserState;
use stacks_core::media::{EpisodeId, MediaId};
use stacks_core::pagination::{cursor_to_page, next_page_cursor};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;
use crate::media::dto::{legacy_status, parse_status};
use crate::media::{ResolveError, ensure_media, find_media};

use super::dto::*;
use super::error::*;

fn status_to_dto(
    cameo_media_id: String,
    entry: &library_entry::Model,
    rewatching: bool,
    runs: &[stacks_core::entity::watch_run::Model],
) -> WatchStatusResponse {
    let first = runs.first();
    let midnight = |d: chrono::NaiveDate| d.and_hms_opt(0, 0, 0).map(|t| t.and_utc().to_rfc3339());
    WatchStatusResponse {
        cameo_media_id,
        status: legacy_status(Some(entry.status), rewatching).unwrap_or_default(),
        score: entry.score.map(i32::from),
        notes: entry.notes.clone(),
        is_favorite: entry.favourite,
        started_at: first.and_then(|r| r.started_on).and_then(midnight),
        completed_at: runs.last().and_then(|r| r.completed_on).and_then(midnight),
        updated_at: entry.updated_at.to_rfc3339(),
    }
}

fn library_error_to_set(e: LibraryError) -> SetStatusError {
    match e {
        LibraryError::InvalidScore => SetStatusError::InvalidScore,
        e => SetStatusError::Internal(crate::internal!(e)),
    }
}

fn resolve_to_set(e: ResolveError) -> SetStatusError {
    match e {
        ResolveError::NotFound => SetStatusError::MediaNotFound,
        ResolveError::Internal(e) => SetStatusError::Internal(crate::internal!(e)),
    }
}

/// Render library rows as media items from their catalog snapshots — no
/// provider call, so an upstream outage cannot empty the library.
async fn to_items(
    state: &AppState,
    views: Vec<EntryView>,
) -> Result<Vec<crate::media::dto::MediaItem>, String> {
    let ids: Vec<_> = views.iter().map(|v| v.media.id).collect();
    let refs = catalog::primary_refs(&state.db, &ids)
        .await
        .map_err(|e| e.to_string())?;
    Ok(views
        .into_iter()
        .filter_map(|v| {
            let mut item = MediaWithUserState::from_snapshot(&v.media, refs.get(&v.media.id)?);
            apply_entry(&mut item, &v.entry, v.progress.as_ref());
            Some(item.into())
        })
        .collect())
}

/// List the authenticated user's tracked media.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "List tracked media (library)")]
pub async fn list_library(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<LibraryResponse>, GetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();

    let pg = crate::extractors::pagination::from_query(req);
    let page = cursor_to_page(pg.cursor.as_deref());
    let per_page = pg.limit_or_default();

    let filter = LibraryFilter {
        status: req
            .query::<String>("status")
            .as_deref()
            .and_then(parse_status),
        kind: req
            .query::<String>("media_type")
            .as_deref()
            .and_then(|k| stacks_core::media::id::parse_kind(k).ok()),
        favourites_only: req.query::<bool>("favorites_only").unwrap_or(false),
    };

    let svc = LibraryService::new(state.db.clone());
    let (views, total_pages) = svc
        .list(user.id, &filter, page, per_page)
        .await
        .map_err(|e| GetStatusError::Internal(crate::internal!(e)))?;
    let items = to_items(&state, views)
        .await
        .map_err(|e| GetStatusError::Internal(crate::internal!(e)))?;

    Ok(Json(LibraryResponse {
        items,
        next_cursor: next_page_cursor(page, total_pages),
    }))
}

async fn current_status(
    state: &AppState,
    user_id: stacks_core::Uuid,
    cameo_id: &str,
    media: &media::Model,
) -> Result<Option<WatchStatusResponse>, String> {
    let svc = LibraryService::new(state.db.clone());
    let Some(view) = svc
        .get(user_id, media.id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let runs = svc
        .runs(user_id, media.id)
        .await
        .map_err(|e| e.to_string())?;
    let rewatching = view.progress.is_some_and(|p| p.run_count > 1);
    Ok(Some(status_to_dto(
        cameo_id.to_owned(),
        &view.entry,
        rewatching,
        &runs,
    )))
}

/// Get watch status for a media item.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Get media watch status")]
pub async fn get_media_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<WatchStatusResponse>, GetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media = find_media(&state, &cameo_id)
        .await
        .map_err(|e| GetStatusError::Internal(crate::internal!(e)))?
        .ok_or(GetStatusError::NotFound)?;
    current_status(&state, user.id, &cameo_id, &media)
        .await
        .map_err(|e| GetStatusError::Internal(crate::internal!(e)))?
        .map(Json)
        .ok_or(GetStatusError::NotFound)
}

/// Parse an RFC 3339 timestamp or a plain `YYYY-MM-DD` date.
fn parse_day(raw: Option<&str>) -> Option<RunDate> {
    let raw = raw?.trim();
    let date = chrono::DateTime::parse_from_rfc3339(raw)
        .map(|d| d.date_naive())
        .or_else(|_| chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d"))
        .ok()?;
    Some(RunDate {
        date,
        precision: DatePrecision::Day,
    })
}

/// Create or update watch status for a media item.
///
/// A merge: fields left out of the body (or sent as `null`) are not changed.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Set media watch status")]
pub async fn set_media_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<WatchStatusResponse>, SetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: WatchStatusRequest = req.parse_json().await.into_internal()?;

    let status = parse_status(&body.status).ok_or(SetStatusError::InvalidStatus)?;
    let score = match body.score {
        Some(s) => Patch::Value(i16::try_from(s).map_err(|_| SetStatusError::InvalidScore)?),
        None => Patch::Absent,
    };
    let media = ensure_media(&state, &cameo_id)
        .await
        .map_err(resolve_to_set)?;
    let svc = LibraryService::new(state.db.clone());

    // "rewatching" on a title already finished starts a new run.
    let existing = svc
        .get(user.id, media.id)
        .await
        .map_err(library_error_to_set)?;
    if body.status == "rewatching"
        && existing
            .as_ref()
            .is_some_and(|v| v.entry.status == EntryStatus::Completed)
    {
        svc.start_rewatch(user.id, media.id, None)
            .await
            .map_err(library_error_to_set)?;
    }

    svc.upsert(
        user.id,
        media.id,
        EntryPatch {
            status: Some(status),
            score,
            favourite: body.is_favorite,
            notes: body.notes.map_or(Patch::Absent, Patch::Value),
        },
    )
    .await
    .map_err(library_error_to_set)?;

    let started = parse_day(body.started_at.as_deref());
    let completed = parse_day(body.completed_at.as_deref());
    if started.is_some() || completed.is_some() {
        let runs = svc
            .runs(user.id, media.id)
            .await
            .map_err(library_error_to_set)?;
        if let Some(run) = runs.last() {
            svc.update_run(
                user.id,
                run.id,
                RunPatch {
                    started: started.map_or(Patch::Absent, Patch::Value),
                    completed: completed.map_or(Patch::Absent, Patch::Value),
                },
            )
            .await
            .map_err(library_error_to_set)?;
        }
    }

    current_status(&state, user.id, &cameo_id, &media)
        .await
        .map_err(|e| SetStatusError::Internal(crate::internal!(e)))?
        .map(Json)
        .ok_or_else(|| SetStatusError::Internal(crate::internal!("entry vanished after upsert")))
}

/// Remove a media item from the library (to the trash; restorable).
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Delete media watch status")]
pub async fn delete_media_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media = find_media(&state, &cameo_id)
        .await
        .map_err(|e| DeleteStatusError::Internal(crate::internal!(e)))?
        .ok_or(DeleteStatusError::NotFound)?;
    LibraryService::new(state.db)
        .delete(user.id, media.id)
        .await
        .map_err(|e| match e {
            LibraryError::NotFound => DeleteStatusError::NotFound,
            e => DeleteStatusError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(()))
}

fn episode_param(req: &Request) -> Option<(String, EpisodeId)> {
    let raw = req.param::<String>("episode_id")?;
    let id = EpisodeId::parse(&raw).ok()?;
    Some((raw, id))
}

fn season_episode(id: &EpisodeId) -> Option<(i32, i32)> {
    Some((
        i32::try_from(id.season).ok()?,
        i32::try_from(id.episode).ok()?,
    ))
}

/// Get watch status for an episode: watched means a live play of it in the
/// current run.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Get episode watch status")]
pub async fn get_episode_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<EpisodeStatusResponse>, GetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let (raw, id) = episode_param(req).ok_or(GetStatusError::NotFound)?;
    let (season, episode) = season_episode(&id).ok_or(GetStatusError::NotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media = find_media(&state, &id.show_id().to_string())
        .await
        .map_err(|e| GetStatusError::Internal(crate::internal!(e)))?
        .ok_or(GetStatusError::NotFound)?;
    let play = LibraryService::new(state.db)
        .episode_play(user.id, media.id, season, episode)
        .await
        .map_err(|e| GetStatusError::Internal(crate::internal!(e)))?
        .ok_or(GetStatusError::NotFound)?;
    Ok(Json(EpisodeStatusResponse {
        cameo_episode_id: raw,
        watched: true,
        progress_seconds: None,
        updated_at: play.updated_at.to_rfc3339(),
    }))
}

/// Mark an episode watched (logs a play) or unwatched (voids its latest
/// play). Nothing is deleted.
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Set episode watch status")]
pub async fn set_episode_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<EpisodeStatusResponse>, SetStatusError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let (raw, id) = episode_param(req).ok_or(SetStatusError::MediaNotFound)?;
    let (season, episode) = season_episode(&id).ok_or(SetStatusError::MediaNotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: EpisodeStatusRequest = req.parse_json().await.into_internal()?;
    let media = ensure_media(&state, &id.show_id().to_string())
        .await
        .map_err(resolve_to_set)?;
    let svc = LibraryService::new(state.db);
    let updated_at = if body.watched {
        svc.log_play(user.id, &media, LogPlay::manual(Some((season, episode))))
            .await
            .map_err(library_error_to_set)?
            .updated_at
    } else {
        match svc
            .unwatch_episode(user.id, media.id, season, episode)
            .await
            .map_err(library_error_to_set)?
        {
            Some(p) => p.updated_at,
            None => stacks_core::db::now(),
        }
    };
    Ok(Json(EpisodeStatusResponse {
        cameo_episode_id: raw,
        watched: body.watched,
        progress_seconds: None,
        updated_at: updated_at.to_rfc3339(),
    }))
}

/// Bulk-update watch status for multiple media items (merge semantics).
#[salvo::oapi::endpoint(tags("Watch State"), summary = "Bulk update watch status")]
pub async fn bulk_update_status(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<BulkStatusResponse>, BulkUpdateError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: BulkStatusRequest = req.parse_json().await.into_internal()?;

    // Validate everything before writing anything.
    let mut planned = Vec::with_capacity(body.entries.len());
    for e in body.entries {
        let status = parse_status(&e.status).ok_or(BulkUpdateError::InvalidStatus)?;
        let score = match e.score {
            Some(s) if (0..=100).contains(&s) => Patch::Value(s as i16),
            Some(_) => return Err(BulkUpdateError::InvalidScore),
            None => Patch::Absent,
        };
        MediaId::parse(&e.cameo_media_id).map_err(|_| BulkUpdateError::InvalidStatus)?;
        planned.push((e.cameo_media_id, status, score));
    }

    let svc = LibraryService::new(state.db.clone());
    let mut updated = 0u32;
    for (cameo_id, status, score) in planned {
        let media = ensure_media(&state, &cameo_id).await.map_err(|e| match e {
            ResolveError::NotFound => BulkUpdateError::InvalidStatus,
            ResolveError::Internal(e) => BulkUpdateError::Internal(crate::internal!(e)),
        })?;
        svc.upsert(
            user.id,
            media.id,
            EntryPatch {
                status: Some(status),
                score,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| match e {
            LibraryError::InvalidScore => BulkUpdateError::InvalidScore,
            e => BulkUpdateError::Internal(crate::internal!(e)),
        })?;
        updated += 1;
    }

    Ok(Json(BulkStatusResponse { updated }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_parse_from_timestamps_and_plain_days() {
        let want = chrono::NaiveDate::from_ymd_opt(2024, 3, 9).unwrap();
        assert_eq!(parse_day(Some("2024-03-09")).unwrap().date, want);
        assert_eq!(
            parse_day(Some("2024-03-09T22:00:00+00:00")).unwrap().date,
            want
        );
        assert!(parse_day(Some("yesterday")).is_none());
        assert!(parse_day(None).is_none());
    }

    #[test]
    fn episode_numbers_fit_the_schema() {
        let id = EpisodeId::parse("tmdb:show:1:2:3").unwrap();
        assert_eq!(season_episode(&id), Some((2, 3)));
        let huge = EpisodeId::parse(&format!("tmdb:show:1:{}:1", u32::MAX)).unwrap();
        assert_eq!(season_episode(&huge), None);
    }

    #[test]
    fn status_dto_reports_run_dates() {
        let now = chrono::Utc::now().into();
        let entry = library_entry::Model {
            id: stacks_core::Uuid::now_v7(),
            user_id: stacks_core::Uuid::now_v7(),
            media_id: stacks_core::Uuid::now_v7(),
            status: EntryStatus::Completed,
            score: Some(90),
            favourite: false,
            notes: None,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };
        let run = stacks_core::entity::watch_run::Model {
            id: stacks_core::Uuid::now_v7(),
            entry_id: entry.id,
            run_number: 1,
            started_on: chrono::NaiveDate::from_ymd_opt(2024, 1, 1),
            started_precision: Some(DatePrecision::Day),
            completed_on: chrono::NaiveDate::from_ymd_opt(2024, 2, 1),
            completed_precision: Some(DatePrecision::Day),
            created_at: now,
            updated_at: now,
        };
        let dto = status_to_dto("tmdb:movie:1".into(), &entry, false, &[run]);
        assert_eq!(dto.status, "completed");
        assert_eq!(dto.score, Some(90));
        assert!(dto.started_at.unwrap().starts_with("2024-01-01"));
        assert!(dto.completed_at.unwrap().starts_with("2024-02-01"));
    }
}
