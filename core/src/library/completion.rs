//! When logging plays completes an entry.
//!
//! A movie's play always does. A show that has stopped airing (finished or
//! cancelled) is finished in a run once that run's live plays cover every
//! episode of every regular season. Specials (season 0) never count, nor do
//! voided plays. A show still airing, or one whose episode list the catalog
//! does not know, is never finished: its status would be wrong by the next
//! episode.

use std::collections::{BTreeSet, HashSet};

use sea_orm::DatabaseTransaction;
use uuid::Uuid;

use crate::entity::enums::{DatePrecision, EntryEventKind, EntryStatus, MediaKind};
use crate::entity::{library_entry, media, play, watch_run};
use crate::media::catalog;
use crate::media::models::in_production;

use super::patch::{RunDate, RunPatch};
use super::{LibraryError, events, plays, repo};

/// Whether the plays just `logged` into `run` complete `entry`: it is not
/// completed yet, and they are a movie's play or they finish a show.
pub(super) async fn completes(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    media: &media::Model,
    entry: &library_entry::Model,
    run: &watch_run::Model,
    logged: &[play::Model],
) -> Result<bool, LibraryError> {
    if entry.status == EntryStatus::Completed {
        return Ok(false);
    }
    if media.kind == MediaKind::Movie {
        return Ok(true);
    }
    finishes_show(txn, user_id, media, run, logged).await
}

/// Whether the plays just `logged` into `run` finish `media`, a show. When
/// they do, the run's completion date becomes the day they were watched,
/// unless it already has one or they carry no date (the status change then
/// stamps today, as for any completion).
///
/// Only the plays that close the last gap finish it. When the run was
/// already finished before them (the user moved the show back off
/// completed), they leave the user's status alone.
async fn finishes_show(
    txn: &DatabaseTransaction,
    user_id: Uuid,
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
    if !required.is_subset(&watched(true)) || required.is_subset(&watched(false)) {
        return Ok(false);
    }
    let watched_at = logged.iter().filter_map(|p| p.watched_at).max();
    if let Some(at) = watched_at
        && run.completed_on.is_none()
    {
        let patch = RunPatch {
            completed: crate::Patch::Value(RunDate {
                date: at.date_naive(),
                precision: DatePrecision::Day,
            }),
            ..Default::default()
        };
        if let Some(am) = patch.to_update(run) {
            let after = repo::update_run(txn, am).await?;
            events::record(
                txn,
                run.entry_id,
                user_id,
                EntryEventKind::RunUpdated,
                Some(run),
                Some(&after),
            )
            .await?;
        }
    }
    Ok(true)
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
