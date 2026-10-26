//! The writes the library's mutations share: making an entry live, patching
//! it, and keeping its runs consistent with its status. Every one runs inside
//! the caller's transaction and writes its own `entry_events` rows.
//!
//! A date the server would otherwise stamp (today, in UTC) can be given
//! instead as `on`: the day the change happened where the user is, as
//! precisely as they know it.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{DatabaseTransaction, TransactionTrait};
use uuid::Uuid;

use crate::entity::enums::{DatePrecision, EntryEventKind, EntryStatus};
use crate::entity::{library_entry, watch_run};
use crate::library::patch::{EntryPatch, RunDate, RunPatch};
use crate::library::{LibraryError, events, repo};

/// Today, known to the day, in UTC: the date a change carries when the
/// request gives none.
pub(super) fn today() -> RunDate {
    RunDate {
        date: chrono::Utc::now().date_naive(),
        precision: DatePrecision::Day,
    }
}

/// The user's live entry for a title, creating it (status `initial`) or
/// restoring it from the trash as needed (writing the matching event), and
/// whether this transaction made it live: not when it was, or a race won.
/// A new entry's runs are dated `on`, or today.
pub(super) async fn ensure_entry(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    media_id: Uuid,
    initial: EntryStatus,
    on: Option<RunDate>,
) -> Result<(library_entry::Model, bool), LibraryError> {
    if let Some(existing) = repo::lock_entry(txn, user_id, media_id).await? {
        return match existing.deleted_at {
            Some(_) => Ok((restore_entry(txn, user_id, existing).await?, true)),
            None => Ok((existing, false)),
        };
    }
    // A concurrent request may insert the same entry first; the savepoint
    // keeps the unique violation from aborting this transaction.
    let sp = txn.begin().await?;
    match repo::insert_entry(&sp, user_id, media_id, initial).await {
        Ok(entry) => {
            sp.commit().await?;
            events::record(
                txn,
                entry.id,
                user_id,
                EntryEventKind::EntryCreated,
                None::<&()>,
                Some(&entry),
            )
            .await?;
            status_side_effects(txn, user_id, &entry, on).await?;
            Ok((entry, true))
        }
        Err(e) if crate::db::is_unique_violation(&e) => {
            sp.rollback().await?;
            let raced = repo::lock_entry(txn, user_id, media_id).await?;
            Ok((raced.ok_or(LibraryError::NotFound)?, false))
        }
        Err(e) => Err(e.into()),
    }
}

pub(super) async fn restore_entry(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry: library_entry::Model,
) -> Result<library_entry::Model, LibraryError> {
    let after = repo::set_deleted(txn, entry.id, false).await?;
    events::record(
        txn,
        entry.id,
        user_id,
        EntryEventKind::EntryRestored,
        Some(&entry),
        Some(&after),
    )
    .await?;
    Ok(after)
}

/// Write a patch (if it changes anything) with its event, then the status
/// side effects: when the status changed, or whenever `on` dates the change,
/// so that "completed, on this day" finishes a run that has no completion
/// even if the entry was completed already.
pub(super) async fn apply_patch(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry: library_entry::Model,
    patch: &EntryPatch,
    on: Option<RunDate>,
) -> Result<library_entry::Model, LibraryError> {
    let after = match patch.to_update(&entry) {
        Some(am) => {
            let after = repo::update_entry(txn, am).await?;
            events::record(
                txn,
                entry.id,
                user_id,
                EntryEventKind::EntryUpdated,
                Some(&entry),
                Some(&after),
            )
            .await?;
            after
        }
        None => entry.clone(),
    };
    if after.status != entry.status || on.is_some() {
        status_side_effects(txn, user_id, &after, on).await?;
    }
    Ok(after)
}

pub(super) async fn set_status(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry: library_entry::Model,
    status: EntryStatus,
    on: Option<RunDate>,
) -> Result<library_entry::Model, LibraryError> {
    let patch = EntryPatch {
        status: Some(status),
        ..Default::default()
    };
    apply_patch(txn, user_id, entry, &patch, on).await
}

/// Keep runs consistent with a status the entry just moved to, dating what
/// it writes `on`, or today:
///
/// - `watching` opens run 1 (started then) when there is no run yet;
/// - `completed` stamps the current run's completion if it has none,
///   opening run 1 first when needed. A given `on` before the run started
///   is [`LibraryError::RunDatesOutOfOrder`].
///
/// Existing dates are never overwritten.
async fn status_side_effects(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry: &library_entry::Model,
    on: Option<RunDate>,
) -> Result<(), LibraryError> {
    let date = on.unwrap_or_else(today);
    match entry.status {
        EntryStatus::Watching => {
            if repo::current_run(txn, entry.id).await?.is_none() {
                insert_run(txn, user_id, entry.id, 1, Some(date), None).await?;
            }
        }
        EntryStatus::Completed => match repo::current_run(txn, entry.id).await? {
            None => {
                insert_run(txn, user_id, entry.id, 1, None, Some(date)).await?;
            }
            Some(run) if run.completed_on.is_none() => {
                let patch = RunPatch {
                    completed: crate::Patch::Value(date),
                    ..Default::default()
                };
                // Only a date the request chose is refused; today is what
                // the server has always stamped.
                if on.is_some() {
                    patch.check(&run)?;
                }
                update_run(txn, user_id, &run, &patch).await?;
            }
            Some(_) => {}
        },
        EntryStatus::Planning | EntryStatus::Paused | EntryStatus::Dropped => {}
    }
    Ok(())
}

/// Plays logged `on` a day landing in `run`: an open run that started
/// later, or never said when, starts on that day instead.
pub(in crate::library) async fn start_by(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    run: &watch_run::Model,
    on: RunDate,
) -> Result<(), LibraryError> {
    if run.completed_on.is_some() || RunDate::started(run).is_some_and(|s| !on.is_before(s)) {
        return Ok(());
    }
    let patch = RunPatch {
        started: crate::Patch::Value(on),
        ..Default::default()
    };
    update_run(txn, user_id, run, &patch).await
}

/// Write `patch` to `run` (if it changes anything) with its event.
pub(super) async fn update_run(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    run: &watch_run::Model,
    patch: &RunPatch,
) -> Result<(), LibraryError> {
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
    Ok(())
}

pub(super) async fn insert_run(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry_id: Uuid,
    run_number: i32,
    started: Option<RunDate>,
    completed: Option<RunDate>,
) -> Result<watch_run::Model, LibraryError> {
    let run = repo::insert_run(
        txn,
        watch_run::ActiveModel {
            id: Set(Uuid::now_v7()),
            entry_id: Set(entry_id),
            run_number: Set(run_number),
            started_on: Set(started.map(|d| d.date)),
            started_precision: Set(started.map(|d| d.precision)),
            completed_on: Set(completed.map(|d| d.date)),
            completed_precision: Set(completed.map(|d| d.precision)),
            created_at: NotSet,
            updated_at: NotSet,
        },
    )
    .await?;
    events::record(
        txn,
        entry_id,
        user_id,
        EntryEventKind::RunStarted,
        None::<&()>,
        Some(&run),
    )
    .await?;
    Ok(run)
}
