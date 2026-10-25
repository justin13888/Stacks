//! The event log, the caller's history, what is up next and what airs when.

use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use stacks_core::library::LibraryService;
use stacks_core::library::schedule::{CalendarPosition, UpNextPosition};
use uuid::Uuid;

use super::entries::stored_title;
use super::{
    CalendarEpisode, CalendarParams, EntryEvent, EntryNotFound, EntryPath, HistoryEvent,
    InvalidDateRange, MAX_CALENDAR_DAYS, UpNextItem, unexpected, window,
};
use crate::catalog::snapshot_summaries;
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::Internal;
use crate::security::{Caller, Reader};

/// What listing an entry's events can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListEntryEventsError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] EntryNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What reading a computed or logged collection can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum FeedError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What reading the calendar can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum CalendarError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidDateRange(#[from] InvalidDateRange),
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// List the changes to one entry, newest first.
///
/// Every change ever made, including those made while the entry was in the
/// trash or before it went there.
#[kynos::get("/library/{ref}/events", operation_id = "listEntryEvents")]
pub(super) async fn list_entry_events(
    Auth(caller): Reader,
    Path(path): Path<EntryPath>,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<EntryEvent>>, ListEntryEventsError> {
    let caller = Caller::from(caller);
    let after: Option<Uuid> = params.after()?;
    let row = stored_title(&db, &path.reference)
        .await?
        .ok_or(EntryNotFound)?;
    let service = LibraryService::new(db);
    let entry = service
        .entry(caller.id(), row.id)
        .await
        .map_err(unexpected)?
        .ok_or(EntryNotFound)?;
    let window = service
        .history(
            caller.id(),
            Some(entry.id),
            after,
            u64::from(params.limit()),
        )
        .await
        .map_err(unexpected)?;
    let page = Page::from_slice(window, |d| page::encode(&d.event.id));
    Ok(Json(page.map(EntryEvent::from)))
}

/// List the caller's history, newest first.
///
/// Every change to every entry, each with the title it concerns.
#[kynos::get("/history", operation_id = "listHistory")]
pub(super) async fn list_history(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<HistoryEvent>>, FeedError> {
    let caller = Caller::from(caller);
    let after: Option<Uuid> = params.after()?;
    let service = LibraryService::new(db.clone());
    let window = service
        .history(caller.id(), None, after, u64::from(params.limit()))
        .await
        .map_err(unexpected)?;
    let window = Page::from_slice(window, |d| page::encode(&d.event.id));
    let mut entry_ids: Vec<Uuid> = window.items.iter().map(|d| d.event.entry_id).collect();
    entry_ids.sort_unstable();
    entry_ids.dedup();
    let entries = service
        .entries_by_ids(&entry_ids)
        .await
        .map_err(unexpected)?;
    let rows: Vec<_> = entries.iter().map(|(_, m)| m.clone()).collect();
    let titles = snapshot_summaries(&db, caller.id(), &rows).await?;
    let title_of: std::collections::HashMap<Uuid, Uuid> =
        entries.iter().map(|(e, m)| (e.id, m.id)).collect();
    let items = window
        .items
        .into_iter()
        .filter_map(|d| {
            let title = titles.get(title_of.get(&d.event.entry_id)?)?.clone();
            Some(HistoryEvent {
                title,
                event: d.into(),
            })
        })
        .collect();
    Ok(Json(Page::new(items, window.next_cursor)))
}

/// List what to watch next.
///
/// For each show the caller is watching, the episode after the furthest one
/// watched in the current run (the first, when none is). The show watched
/// most recently comes first; a show with nothing left to watch is not
/// listed.
#[kynos::get("/up-next", operation_id = "listUpNext")]
pub(super) async fn list_up_next(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<UpNextItem>>, FeedError> {
    let caller = Caller::from(caller);
    // Check the cursor before doing the work it would page.
    params.after::<UpNextPosition>()?;
    let all = LibraryService::new(db.clone())
        .up_next(caller.id())
        .await
        .map_err(unexpected)?;
    let page = window(all, |item| item.position(), &params)?;
    let rows: Vec<_> = page.items.iter().map(|i| i.media.clone()).collect();
    let titles = snapshot_summaries(&db, caller.id(), &rows).await?;
    let today = chrono::Utc::now().date_naive();
    let items = page
        .items
        .into_iter()
        .filter_map(|i| {
            let air_date = i.episode.as_ref().and_then(|e| e.air_date);
            Some(UpNextItem {
                title: titles.get(&i.media.id)?.clone(),
                season: u32::try_from(i.season).ok()?,
                episode: u32::try_from(i.number).ok()?,
                name: i.episode.as_ref().and_then(|e| e.title.clone()),
                air_date,
                aired: air_date.is_some_and(|d| d <= today),
                last_watched_at: i.last_watched_at.map(|t| t.to_utc()),
            })
        })
        .collect();
    Ok(Json(Page::new(items, page.next_cursor)))
}

/// List the episodes airing in a date range.
///
/// Episodes of the shows in the caller's library (any status but
/// `dropped`), from `from` to `to` inclusive, ordered by air date and then
/// title, each saying whether the caller has watched it. The range spans at
/// most 62 days.
#[kynos::get("/calendar", operation_id = "listCalendar")]
pub(super) async fn list_calendar(
    Auth(caller): Reader,
    Query(params): Query<CalendarParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<CalendarEpisode>>, CalendarError> {
    let caller = Caller::from(caller);
    let span = (params.to - params.from).num_days();
    if !(0..=MAX_CALENDAR_DAYS).contains(&span) {
        return Err(InvalidDateRange.into());
    }
    let paging = PageParams {
        cursor: params.cursor,
        limit: params.limit,
    };
    paging.after::<CalendarPosition>()?;
    let all = LibraryService::new(db.clone())
        .calendar(caller.id(), params.from, params.to)
        .await
        .map_err(unexpected)?;
    let page = window(all, |item| item.position(), &paging)?;
    let rows: Vec<_> = page.items.iter().map(|i| i.media.clone()).collect();
    let titles = snapshot_summaries(&db, caller.id(), &rows).await?;
    let items = page
        .items
        .into_iter()
        .filter_map(|i| {
            Some(CalendarEpisode {
                title: titles.get(&i.media.id)?.clone(),
                season: u32::try_from(i.season).ok()?,
                episode: u32::try_from(i.episode.number).ok()?,
                name: i.episode.title.clone(),
                air_date: i.air_date,
                runtime_minutes: i
                    .episode
                    .runtime_minutes
                    .and_then(|r| u32::try_from(r).ok()),
                watched: i.watched,
            })
        })
        .collect();
    Ok(Json(Page::new(items, page.next_cursor)))
}
