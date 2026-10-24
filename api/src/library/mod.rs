//! The caller's library: entries, watch runs, plays, the event log, and what
//! is up next or airing.
//!
//! Every title is named by its [`TitleRef`](crate::ids::TitleRef) and every
//! stored title renders from its catalog snapshot, so the library reads
//! without calling a provider. Mutations go through
//! [`stacks_core::library::LibraryService`], each one transaction with its
//! event-log rows; nothing here deletes a row (entries go to the trash,
//! plays are voided, and both can be restored).

mod dto;
mod entries;
mod feeds;
mod plays;

use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use serde::de::DeserializeOwned;
use stacks_core::library::{EntryView, LibraryError};
use uuid::Uuid;

pub use dto::*;

use crate::app::App;
use crate::catalog::{TitleSummary, snapshot_summaries};
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::Internal;

/// The caller's library.
#[derive(Tag)]
#[tag(
    name = "library",
    description = "Entries, watch runs and plays; the event log and history; what is up next and what airs when."
)]
pub struct Library;

// ----------------------------------------------------------------- errors

/// The title is not in the caller's library (or is in its trash).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the title is not in your library")]
#[problem(base = crate::problem::BASE, status = 404, title = "Entry not found")]
pub struct EntryNotFound;

/// The title is not in the caller's trash.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the title is not in your trash")]
#[problem(base = crate::problem::BASE, status = 404, title = "Entry not in the trash")]
pub struct TrashedEntryNotFound;

/// The entry has no run with that number.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the entry has no run with that number")]
#[problem(base = crate::problem::BASE, status = 404, title = "Watch run not found")]
pub struct WatchRunNotFound;

/// The show has no such episode.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the show has no episode {0}")]
#[problem(base = crate::problem::BASE, status = 404, title = "Episode not found")]
pub struct EpisodeNotFound(pub String);

/// The play kind does not fit the title: episodes for a movie, or a movie
/// play for a show.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{0}")]
#[problem(base = crate::problem::BASE, status = 422, title = "Wrong play kind")]
pub struct WrongPlayKind(pub &'static str);

/// No play of the caller's has that id.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no play of yours has that id")]
#[problem(base = crate::problem::BASE, status = 404, title = "Play not found")]
pub struct PlayNotFound;

/// The play was voided already; `restorePlay` brings it back.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the play is already voided")]
#[problem(base = crate::problem::BASE, status = 409, title = "Play already voided")]
pub struct PlayAlreadyVoided;

/// The play is not voided, so there is nothing to restore.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the play is not voided")]
#[problem(base = crate::problem::BASE, status = 409, title = "Play not voided")]
pub struct PlayNotVoided;

/// The calendar range is backwards or longer than [`MAX_CALENDAR_DAYS`].
#[derive(Debug, thiserror::Error, ApiError)]
#[error("`to` must be on or after `from`, and at most 62 days after it")]
#[problem(base = crate::problem::BASE, status = 422, title = "Invalid date range")]
pub struct InvalidDateRange;

/// The longest span `listCalendar` covers, in days between `from` and `to`.
pub const MAX_CALENDAR_DAYS: i64 = 62;

/// A core failure no operation expects at that point: logged, and a 500.
#[track_caller]
fn unexpected(e: LibraryError) -> Internal {
    Internal::log(e)
}

// ---------------------------------------------------------------- helpers

/// The summaries of the titles of `views`, rendered as entries, in order.
/// A title without a summary (a row with no public reference) is left out.
async fn render_entries(
    db: &DatabaseConnection,
    user_id: Uuid,
    views: Vec<EntryView>,
) -> Result<Vec<LibraryEntry>, Internal> {
    let rows: Vec<_> = views.iter().map(|v| v.media.clone()).collect();
    let summaries = snapshot_summaries(db, user_id, &rows).await?;
    Ok(views
        .iter()
        .filter_map(|v| Some(LibraryEntry::new(summaries.get(&v.media.id)?.clone(), v)))
        .collect())
}

/// One entry rendered, or a 500 when its title cannot be named (which the
/// catalog's invariants rule out).
async fn render_entry(
    db: &DatabaseConnection,
    user_id: Uuid,
    view: EntryView,
) -> Result<LibraryEntry, Internal> {
    render_entries(db, user_id, vec![view])
        .await?
        .pop()
        .ok_or_else(|| Internal::log("a library entry whose title has no reference"))
}

/// The summary of each stored title, keyed by its row id.
async fn summaries(
    db: &DatabaseConnection,
    user_id: Uuid,
    rows: &[stacks_core::entity::media::Model],
) -> Result<std::collections::HashMap<Uuid, TitleSummary>, Internal> {
    snapshot_summaries(db, user_id, rows).await
}

/// Page a list already held in memory and sorted ascending by `key`,
/// continuing strictly after the cursor's position: a keyset, not an offset.
fn window<T, K>(
    items: Vec<T>,
    key: impl Fn(&T) -> K,
    params: &PageParams,
) -> Result<Page<T>, InvalidCursor>
where
    K: Ord + Serialize + DeserializeOwned,
{
    let after: Option<K> = params.after()?;
    let items = items
        .into_iter()
        .filter(|item| after.as_ref().is_none_or(|a| key(item) > *a))
        .collect();
    Ok(page::take(items, params.limit(), |last| {
        page::encode(&key(last))
    }))
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Library>().mount(kynos::routes![
        entries::list_library,
        entries::get_library_entry,
        entries::put_library_entry,
        entries::update_library_entry,
        entries::delete_library_entry,
        entries::restore_library_entry,
        entries::start_rewatch,
        entries::update_watch_run,
        plays::log_plays,
        plays::void_play,
        plays::restore_play,
        feeds::list_entry_events,
        feeds::list_history,
        feeds::list_up_next,
        feeds::list_calendar,
    ])
}
