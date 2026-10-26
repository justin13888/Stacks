//! When logging plays completes an entry.
//!
//! A movie's play always does. A show that has stopped airing (finished or
//! cancelled) is finished in a run once that run's live plays cover every
//! episode of every regular season. Specials (season 0) never count, nor do
//! voided plays. A show still airing, or one whose episode list the catalog
//! does not know, is never finished: its status would be wrong by the next
//! episode. A run completes this way at most once: a status the user sets
//! after it finished is never overridden.

use std::collections::{BTreeSet, HashSet};

use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, DatabaseTransaction, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::enums::{DatePrecision, EntryEventKind, EntryStatus, MediaKind};
use crate::entity::{entry_event, library_entry, media, play, watch_run};
use crate::media::catalog;
use crate::media::models::in_production;

use super::patch::RunDate;
use super::service::writes::start_by;
use super::{LibraryError, plays};

/// Whether the plays just `logged` into `run` complete `entry`.
///
/// A movie's play does unless the entry is completed already; a play dated
/// `on` completes even then, finishing an open rewatch run. A show's plays
/// do when they [finish](finishes_show) it, at most once per run.
pub(super) async fn completes(
    txn: &DatabaseTransaction,
    media: &media::Model,
    entry: &library_entry::Model,
    run: &watch_run::Model,
    logged: &[play::Model],
    on: Option<RunDate>,
) -> Result<bool, LibraryError> {
    if media.kind == MediaKind::Movie {
        return Ok(entry.status != EntryStatus::Completed || on.is_some());
    }
    if entry.status == EntryStatus::Completed || completed_before(txn, run).await? {
        return Ok(false);
    }
    finishes_show(txn, media, run, logged).await
}

/// The day the plays that complete an entry finished `run`, for the
/// `completed` status change to stamp: `on`, as precisely as the caller knew
/// it, or else the UTC date of `watched_at`, or else (`None`) today.
///
/// A run that started after a `watched_at` date starts on it instead, so
/// the completion never precedes the start. One that never said when it
/// started keeps saying nothing.
pub(super) async fn finished_on(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    run: &watch_run::Model,
    on: Option<RunDate>,
    watched_at: Option<DateTimeWithTimeZone>,
) -> Result<Option<RunDate>, LibraryError> {
    if on.is_some() {
        return Ok(on);
    }
    let Some(at) = watched_at else {
        return Ok(None);
    };
    let day = RunDate {
        date: at.to_utc().date_naive(),
        precision: DatePrecision::Day,
    };
    if RunDate::started(run).is_some_and(|s| day.is_before(s)) {
        start_by(txn, user_id, run, day).await?;
    }
    Ok(Some(day))
}

/// Whether `run` was ever completed: it has a completion date, or one of
/// its events records it with one (a date the user later cleared).
async fn completed_before(
    txn: &DatabaseTransaction,
    run: &watch_run::Model,
) -> Result<bool, LibraryError> {
    if run.completed_on.is_some() {
        return Ok(true);
    }
    let found = entry_event::Entity::find()
        .filter(entry_event::Column::EntryId.eq(run.entry_id))
        .filter(
            entry_event::Column::Kind.is_in([EntryEventKind::RunStarted, EntryEventKind::RunUpdated]),
        )
        .filter(Expr::cust_with_values(
            "entry_events.after ->> 'id' = $1 AND entry_events.after ->> 'completed_on' IS NOT NULL",
            [run.id.to_string()],
        ))
        .count(txn)
        .await?;
    Ok(found > 0)
}

/// Whether the plays just `logged` into `run` finish `media`, a show that
/// has stopped airing: with them, the run's live plays cover every regular
/// episode, and without them they did not.
async fn finishes_show(
    txn: &DatabaseTransaction,
    media: &media::Model,
    run: &watch_run::Model,
    logged: &[play::Model],
) -> Result<bool, LibraryError> {
    // Neither upcoming, airing nor on hiatus, and known to be so.
    if media.kind != MediaKind::Show || in_production(media.status) != Some(false) {
        return Ok(false);
    }
    let Some(required) = regular_episodes(txn, media).await? else {
        return Ok(false);
    };
    let new: HashSet<Uuid> = logged.iter().map(|p| p.id).collect();
    let live = plays::live_plays_in_runs(txn, &[run.id]).await?;
    let ids: Vec<Uuid> = live.iter().filter_map(|p| p.episode_id).collect();
    let numbers = catalog::episode_numbers(txn, &ids).await?;
    let watched = |include_new: bool| -> BTreeSet<(i32, i32)> {
        live.iter()
            .filter(|p| include_new || !new.contains(&p.id))
            .filter_map(|p| p.episode_id.and_then(|e| numbers.get(&e)).copied())
            .collect()
    };
    Ok(required.is_subset(&watched(true)) && !required.is_subset(&watched(false)))
}

/// Every `(season, episode)` of the show's regular seasons, or `None` when
/// the catalog cannot say: no regular season stored, fewer stored than the
/// show has, or one whose episode count is unknown.
async fn regular_episodes(
    txn: &DatabaseTransaction,
    media: &media::Model,
) -> Result<Option<BTreeSet<(i32, i32)>>, LibraryError> {
    let seasons: Vec<_> = catalog::seasons_of(txn, media.id)
        .await?
        .into_iter()
        .filter(|s| s.number >= 1)
        .collect();
    let expected = media
        .season_count
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(1);
    if seasons.len() < expected.max(1) {
        return Ok(None);
    }
    let mut required = BTreeSet::new();
    for season in seasons {
        match season.episode_count {
            Some(n) if n >= 1 => required.extend((1..=n).map(|e| (season.number, e))),
            _ => return Ok(None),
        }
    }
    Ok(Some(required))
}
