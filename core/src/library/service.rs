//! Library mutations. Each public method is one transaction that also writes
//! the `entry_events` row(s) describing what it changed; any failure rolls
//! back the change and its events together.

use std::collections::HashMap;

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, TransactionTrait};
use uuid::Uuid;

use crate::entity::enums::{DatePrecision, EntryEventKind, EntryStatus, MediaKind, PlaySource};
use crate::entity::{entry_progress, library_entry, media, play, tag, watch_run};
use crate::media::catalog;
use crate::pagination::Slice;

use super::events::{Described, EntryState};
use super::listing::{self, EntryPosition, LibraryFilter, LibrarySort};
use super::patch::{EntryPatch, RunDate, RunPatch};
use super::schedule::{self, CalendarEpisode, UpNext};
use super::{LibraryError, events, plays, repo};

/// An entry with its catalog snapshot and derived progress.
#[derive(Debug, Clone)]
pub struct EntryView {
    pub entry: library_entry::Model,
    pub media: media::Model,
    /// `None` only if the view row is missing, which the view's definition
    /// rules out for a stored entry.
    pub progress: Option<entry_progress::Model>,
    /// The highest-numbered run; `None` before the first play or run.
    pub current_run: Option<watch_run::Model>,
    /// The tags the entry carries, by name.
    pub tags: Vec<tag::Model>,
}

#[cfg(feature = "test-support")]
pub mod test_support;

/// The plays one request logs against a title, in one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayBatch {
    /// One play of a movie.
    Movie,
    /// One play of each listed `(season, episode)`, in order.
    Episodes(Vec<(i32, i32)>),
    /// One play of each listed `(season, episode)` not already watched in
    /// the current run.
    Unwatched(Vec<(i32, i32)>),
}

/// An episode watched in a run, with when it was last watched and the live
/// plays that make it watched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedEpisode {
    pub season: i32,
    pub episode: i32,
    pub watched_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    /// The ids of its live plays in the run, oldest first. Voiding every one
    /// makes the episode unwatched.
    pub plays: Vec<Uuid>,
}

pub struct LibraryService {
    db: DatabaseConnection,
}

fn today() -> RunDate {
    RunDate {
        date: chrono::Utc::now().date_naive(),
        precision: DatePrecision::Day,
    }
}

impl LibraryService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    // ------------------------------------------------------------------ reads

    /// The user's live entry for a title.
    pub async fn get(
        &self,
        user_id: Uuid,
        media_id: Uuid,
    ) -> Result<Option<EntryView>, LibraryError> {
        let Some(entry) = repo::find_entry(&self.db, user_id, media_id)
            .await?
            .filter(|e| e.deleted_at.is_none())
        else {
            return Ok(None);
        };
        let media = media_row(&self.db, entry.media_id).await?;
        let progress = repo::progress(&self.db, entry.id).await?;
        let current_run = repo::current_run(&self.db, entry.id).await?;
        let tags = repo::tags_for_entries(&self.db, &[entry.id])
            .await?
            .remove(&entry.id)
            .unwrap_or_default();
        Ok(Some(EntryView {
            entry,
            media,
            progress,
            current_run,
            tags,
        }))
    }

    /// One window of the library (or of the trash), continuing after
    /// `after`, which must be a position of `sort`.
    pub async fn list(
        &self,
        user_id: Uuid,
        filter: &LibraryFilter,
        sort: LibrarySort,
        after: Option<&EntryPosition>,
        limit: u64,
    ) -> Result<Slice<EntryView>, LibraryError> {
        let rows = listing::list_entries(&self.db, user_id, filter, sort, after, limit).await?;
        let more = rows.more;
        let items = self.attach_progress(rows.items).await?;
        Ok(Slice { items, more })
    }

    /// Every live entry (for stats and export).
    pub async fn all(&self, user_id: Uuid) -> Result<Vec<EntryView>, LibraryError> {
        let rows = repo::all_entries(&self.db, user_id).await?;
        self.attach_progress(rows).await
    }

    /// Soft-deleted entries, most recently deleted first.
    pub async fn trash(&self, user_id: Uuid) -> Result<Vec<EntryView>, LibraryError> {
        let rows = repo::list_trash(&self.db, user_id).await?;
        self.attach_progress(rows).await
    }

    async fn attach_progress(
        &self,
        rows: Vec<(library_entry::Model, media::Model)>,
    ) -> Result<Vec<EntryView>, LibraryError> {
        let ids: Vec<Uuid> = rows.iter().map(|(e, _)| e.id).collect();
        let mut runs = repo::current_runs(&self.db, &ids).await?;
        let mut tags = repo::tags_for_entries(&self.db, &ids).await?;
        let mut progress: HashMap<Uuid, entry_progress::Model> = repo::progress_many(&self.db, ids)
            .await?
            .into_iter()
            .map(|p| (p.entry_id, p))
            .collect();
        Ok(rows
            .into_iter()
            .map(|(entry, media)| EntryView {
                progress: progress.remove(&entry.id),
                current_run: runs.remove(&entry.id),
                tags: tags.remove(&entry.id).unwrap_or_default(),
                entry,
                media,
            })
            .collect())
    }

    /// The runs of the user's entry for a title, first watch first.
    pub async fn runs(
        &self,
        user_id: Uuid,
        media_id: Uuid,
    ) -> Result<Vec<watch_run::Model>, LibraryError> {
        let entry = repo::find_entry(&self.db, user_id, media_id)
            .await?
            .ok_or(LibraryError::NotFound)?;
        Ok(repo::runs_for_entry(&self.db, entry.id).await?)
    }

    /// Every play of the user's entry for a title, voided ones included.
    pub async fn plays(
        &self,
        user_id: Uuid,
        media_id: Uuid,
    ) -> Result<Vec<play::Model>, LibraryError> {
        let entry = repo::find_entry(&self.db, user_id, media_id)
            .await?
            .ok_or(LibraryError::NotFound)?;
        Ok(plays::plays_for_entry(&self.db, entry.id).await?)
    }

    /// The latest live play of an episode in the current run, if any — what
    /// "is this episode watched?" means.
    pub async fn episode_play(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        season: i32,
        episode: i32,
    ) -> Result<Option<play::Model>, LibraryError> {
        let Some(entry) = repo::find_entry(&self.db, user_id, media_id).await? else {
            return Ok(None);
        };
        let Some(run) = repo::current_run(&self.db, entry.id).await? else {
            return Ok(None);
        };
        let Some(ep) = catalog::find_episode(&self.db, media_id, season, episode).await? else {
            return Ok(None);
        };
        Ok(plays::latest_active_episode_play(&self.db, run.id, ep.id).await?)
    }

    /// The user's entry for a title in any state, soft-deleted included.
    pub async fn entry(
        &self,
        user_id: Uuid,
        media_id: Uuid,
    ) -> Result<Option<library_entry::Model>, LibraryError> {
        Ok(repo::find_entry(&self.db, user_id, media_id).await?)
    }

    /// The episodes watched in an entry's current run, in order, each with
    /// when it was last watched.
    pub async fn watched_in_current_run(
        &self,
        entry_id: Uuid,
    ) -> Result<Vec<WatchedEpisode>, LibraryError> {
        let Some(run) = repo::current_run(&self.db, entry_id).await? else {
            return Ok(Vec::new());
        };
        let live = plays::live_plays_in_runs(&self.db, &[run.id]).await?;
        let ids: Vec<Uuid> = live.iter().filter_map(|p| p.episode_id).collect();
        let numbers = catalog::episode_numbers(&self.db, &ids).await?;
        let mut latest: std::collections::BTreeMap<(i32, i32), WatchedEpisode> =
            std::collections::BTreeMap::new();
        for p in live {
            let Some(&(season, episode)) = p.episode_id.and_then(|e| numbers.get(&e)) else {
                continue;
            };
            let slot = latest.entry((season, episode)).or_insert(WatchedEpisode {
                season,
                episode,
                watched_at: None,
                plays: Vec::new(),
            });
            if p.watched_at > slot.watched_at {
                slot.watched_at = p.watched_at;
            }
            // `live` is oldest first, so the ids are too.
            slot.plays.push(p.id);
        }
        Ok(latest.into_values().collect())
    }

    /// One window of the user's events, newest first, optionally for one
    /// entry, continuing after the event `after`.
    pub async fn history(
        &self,
        user_id: Uuid,
        entry_id: Option<Uuid>,
        after: Option<Uuid>,
        limit: u64,
    ) -> Result<Slice<Described>, LibraryError> {
        let window = events::list(&self.db, user_id, entry_id, after, limit).await?;
        let more = window.more;
        let items = events::describe(&self.db, window.items).await?;
        Ok(Slice { items, more })
    }

    /// The entries (any state) and snapshots the given entry ids name.
    pub async fn entries_by_ids(
        &self,
        entry_ids: &[Uuid],
    ) -> Result<Vec<(library_entry::Model, media::Model)>, LibraryError> {
        Ok(repo::entries_by_ids(&self.db, entry_ids).await?)
    }

    /// For each show the user is watching, the next episode to watch.
    pub async fn up_next(&self, user_id: Uuid) -> Result<Vec<UpNext>, LibraryError> {
        Ok(schedule::up_next(&self.db, user_id).await?)
    }

    /// The episodes airing from `from` to `to` for the user's shows.
    pub async fn calendar(
        &self,
        user_id: Uuid,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> Result<Vec<CalendarEpisode>, LibraryError> {
        Ok(schedule::calendar(&self.db, user_id, from, to).await?)
    }

    // -------------------------------------------------------------- mutations

    /// Add a title to the library (or restore it from the trash) and apply
    /// `patch`. A new entry starts as `planning` unless the patch says
    /// otherwise.
    pub async fn upsert(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        patch: EntryPatch,
    ) -> Result<library_entry::Model, LibraryError> {
        patch.validate()?;
        let txn = self.db.begin().await?;
        let initial = patch.status.unwrap_or(EntryStatus::Planning);
        let (entry, _) = ensure_entry(&txn, user_id, media_id, initial).await?;
        let entry = apply_patch(&txn, user_id, entry, &patch).await?;
        txn.commit().await?;
        Ok(entry)
    }

    /// Set an entry to exactly `state`, adding the title (or restoring it from
    /// the trash) first if need be, and say whether it was not live before.
    /// With `only_if_absent`, a live entry is [`LibraryError::AlreadyLive`].
    pub async fn replace(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        state: EntryState,
        only_if_absent: bool,
    ) -> Result<(library_entry::Model, bool), LibraryError> {
        let patch = EntryPatch {
            status: Some(state.status),
            score: Some(state.score).into(),
            favourite: Some(state.favourite),
            notes: Some(state.notes).into(),
        };
        patch.validate()?;
        let txn = self.db.begin().await?;
        let (entry, made_live) = ensure_entry(&txn, user_id, media_id, state.status).await?;
        if only_if_absent && !made_live {
            return Err(LibraryError::AlreadyLive);
        }
        let entry = apply_patch(&txn, user_id, entry, &patch).await?;
        txn.commit().await?;
        Ok((entry, made_live))
    }

    /// Change an existing live entry. Absent fields are left untouched.
    pub async fn patch(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        patch: EntryPatch,
    ) -> Result<library_entry::Model, LibraryError> {
        patch.validate()?;
        let txn = self.db.begin().await?;
        let entry = repo::lock_entry(&txn, user_id, media_id)
            .await?
            .filter(|e| e.deleted_at.is_none())
            .ok_or(LibraryError::NotFound)?;
        let entry = apply_patch(&txn, user_id, entry, &patch).await?;
        txn.commit().await?;
        Ok(entry)
    }

    /// Move an entry to the trash. Its runs, plays, tags and history stay.
    pub async fn delete(&self, user_id: Uuid, media_id: Uuid) -> Result<(), LibraryError> {
        let txn = self.db.begin().await?;
        let entry = repo::lock_entry(&txn, user_id, media_id)
            .await?
            .filter(|e| e.deleted_at.is_none())
            .ok_or(LibraryError::NotFound)?;
        let after = repo::set_deleted(&txn, entry.id, true).await?;
        events::record(
            &txn,
            entry.id,
            user_id,
            EntryEventKind::EntryDeleted,
            Some(&entry),
            Some(&after),
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    /// Bring a trashed entry back exactly as it was (a live one is `AlreadyLive`).
    pub async fn restore(
        &self,
        user_id: Uuid,
        media_id: Uuid,
    ) -> Result<library_entry::Model, LibraryError> {
        let txn = self.db.begin().await?;
        let entry = repo::lock_entry(&txn, user_id, media_id)
            .await?
            .ok_or(LibraryError::NotFound)?;
        if entry.deleted_at.is_none() {
            return Err(LibraryError::AlreadyLive);
        }
        let after = restore_entry(&txn, user_id, entry).await?;
        txn.commit().await?;
        Ok(after)
    }

    /// Start a rewatch: a new run, and the entry back to `watching`.
    pub async fn start_rewatch(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        started: Option<RunDate>,
    ) -> Result<watch_run::Model, LibraryError> {
        let txn = self.db.begin().await?;
        let entry = repo::lock_entry(&txn, user_id, media_id)
            .await?
            .filter(|e| e.deleted_at.is_none())
            .ok_or(LibraryError::NotFound)?;
        let next = repo::current_run(&txn, entry.id)
            .await?
            .map_or(1, |r| r.run_number + 1);
        let run = insert_run(
            &txn,
            user_id,
            entry.id,
            next,
            Some(started.unwrap_or_else(today)),
            None,
        )
        .await?;
        set_status(&txn, user_id, entry, EntryStatus::Watching).await?;
        txn.commit().await?;
        Ok(run)
    }

    /// Change a run's dates. Absent fields are left untouched.
    pub async fn update_run(
        &self,
        user_id: Uuid,
        run_id: Uuid,
        patch: RunPatch,
    ) -> Result<watch_run::Model, LibraryError> {
        let txn = self.db.begin().await?;
        let run = repo::find_run(&txn, run_id)
            .await?
            .ok_or(LibraryError::RunNotFound)?;
        owned_entry(&txn, user_id, run.entry_id)
            .await?
            .ok_or(LibraryError::RunNotFound)?;
        let Some(am) = patch.to_update(&run) else {
            return Ok(run);
        };
        let after = repo::update_run(&txn, am).await?;
        events::record(
            &txn,
            run.entry_id,
            user_id,
            EntryEventKind::RunUpdated,
            Some(&run),
            Some(&after),
        )
        .await?;
        txn.commit().await?;
        Ok(after)
    }

    /// Change the dates of run `number` of the user's live entry for a
    /// title. Absent fields are left untouched.
    pub async fn update_run_by_number(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        number: i32,
        patch: RunPatch,
    ) -> Result<watch_run::Model, LibraryError> {
        let entry = repo::find_entry(&self.db, user_id, media_id)
            .await?
            .filter(|e| e.deleted_at.is_none())
            .ok_or(LibraryError::NotFound)?;
        let run = repo::run_by_number(&self.db, entry.id, number)
            .await?
            .ok_or(LibraryError::RunNotFound)?;
        self.update_run(user_id, run.id, patch).await
    }

    /// Record a batch of plays in one transaction, with one `play_logged`
    /// event each.
    ///
    /// Tracks the title if it is not in the library yet (restoring it from
    /// the trash if need be), opens run 1 if there is none, and moves a
    /// `planning` entry to `watching`. A movie play completes the entry.
    /// [`PlayBatch::Unwatched`] skips episodes already watched in the
    /// current run, and when that leaves nothing, nothing changes at all.
    pub async fn log_plays(
        &self,
        user_id: Uuid,
        media: &media::Model,
        batch: PlayBatch,
        watched_at: Option<sea_orm::prelude::DateTimeWithTimeZone>,
        source: PlaySource,
        import_id: Option<Uuid>,
    ) -> Result<Vec<play::Model>, LibraryError> {
        let (episodes, skip_watched) = match batch {
            PlayBatch::Movie if media.kind == MediaKind::Movie => (vec![None], false),
            PlayBatch::Movie => return Err(LibraryError::NotAMovie),
            _ if media.kind != MediaKind::Show => return Err(LibraryError::NotAShow),
            PlayBatch::Episodes(e) => (e.into_iter().map(Some).collect::<Vec<_>>(), false),
            PlayBatch::Unwatched(e) => (e.into_iter().map(Some).collect::<Vec<_>>(), true),
        };
        let txn = self.db.begin().await?;
        let index = catalog::episodes_of_many(&txn, &[media.id])
            .await?
            .remove(&media.id)
            .unwrap_or_default();
        let index: HashMap<(i32, i32), Uuid> = index
            .into_iter()
            .map(|(s, e)| ((s, e.number), e.id))
            .collect();
        let mut episode_ids: Vec<Option<Uuid>> = Vec::with_capacity(episodes.len());
        for episode in &episodes {
            episode_ids.push(match episode {
                Some(key) => Some(match index.get(key) {
                    Some(id) => *id,
                    None => {
                        catalog::ensure_episode(&txn, media.id, key.0, key.1)
                            .await?
                            .id
                    }
                }),
                None => None,
            });
        }
        // A trashed entry counts too: `ensure_entry` restores it with the
        // same current run, so what that run has seen is still seen.
        if skip_watched
            && let Some(entry) = repo::lock_entry(&txn, user_id, media.id).await?
            && let Some(run) = repo::current_run(&txn, entry.id).await?
        {
            let seen: std::collections::HashSet<Uuid> = plays::live_plays_in_runs(&txn, &[run.id])
                .await?
                .into_iter()
                .filter_map(|p| p.episode_id)
                .collect();
            episode_ids.retain(|e| e.is_none_or(|id| !seen.contains(&id)));
        }
        if episode_ids.is_empty() {
            txn.commit().await?;
            return Ok(Vec::new());
        }
        let (mut entry, _) = ensure_entry(&txn, user_id, media.id, EntryStatus::Watching).await?;
        if entry.status == EntryStatus::Planning {
            entry = set_status(&txn, user_id, entry, EntryStatus::Watching).await?;
        }
        let run = match repo::current_run(&txn, entry.id).await? {
            Some(r) => r,
            None => insert_run(&txn, user_id, entry.id, 1, Some(today()), None).await?,
        };
        let now = crate::db::now();
        let logged: Vec<play::Model> = episode_ids
            .into_iter()
            .map(|episode_id| play::Model {
                id: Uuid::now_v7(),
                run_id: run.id,
                episode_id,
                watched_at,
                source,
                import_id,
                voided_at: None,
                created_at: now,
                updated_at: now,
            })
            .collect();
        plays::insert_plays(&txn, &logged).await?;
        let afters: Vec<(Uuid, &play::Model)> = logged.iter().map(|p| (entry.id, p)).collect();
        events::record_many(&txn, user_id, EntryEventKind::PlayLogged, &afters).await?;
        if media.kind == MediaKind::Movie && entry.status != EntryStatus::Completed {
            set_status(&txn, user_id, entry, EntryStatus::Completed).await?;
        }
        txn.commit().await?;
        Ok(logged)
    }

    /// Undo a play by voiding it. The play row is kept.
    pub async fn void_play(
        &self,
        user_id: Uuid,
        play_id: Uuid,
    ) -> Result<play::Model, LibraryError> {
        let txn = self.db.begin().await?;
        let (play, entry_id) = owned_play(&txn, user_id, play_id).await?;
        if play.voided_at.is_some() {
            return Err(LibraryError::AlreadyVoided);
        }
        let after = plays::void_play(&txn, play.id).await?;
        events::record(
            &txn,
            entry_id,
            user_id,
            EntryEventKind::PlayVoided,
            Some(&play),
            Some(&after),
        )
        .await?;
        txn.commit().await?;
        Ok(after)
    }

    /// Undo a void: the play counts again, exactly as it was logged (same
    /// id, run, episode, time and source).
    pub async fn restore_play(
        &self,
        user_id: Uuid,
        play_id: Uuid,
    ) -> Result<play::Model, LibraryError> {
        let txn = self.db.begin().await?;
        let (play, entry_id) = owned_play(&txn, user_id, play_id).await?;
        if play.voided_at.is_none() {
            return Err(LibraryError::NotVoided);
        }
        let after = plays::restore_play(&txn, play.id).await?;
        events::record(
            &txn,
            entry_id,
            user_id,
            EntryEventKind::PlayRestored,
            Some(&play),
            Some(&after),
        )
        .await?;
        txn.commit().await?;
        Ok(after)
    }
}

async fn media_row(db: &DatabaseConnection, media_id: Uuid) -> Result<media::Model, LibraryError> {
    media::Entity::find_by_id(media_id)
        .one(db)
        .await?
        .ok_or_else(|| LibraryError::Db(DbErr::RecordNotFound(media_id.to_string())))
}

/// A play of the user's, locked, with the id of the entry it belongs to.
/// Someone else's play is [`LibraryError::PlayNotFound`], like a missing one.
async fn owned_play(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    play_id: Uuid,
) -> Result<(play::Model, Uuid), LibraryError> {
    let play = plays::lock_play(txn, play_id)
        .await?
        .ok_or(LibraryError::PlayNotFound)?;
    let run = repo::find_run(txn, play.run_id)
        .await?
        .ok_or(LibraryError::PlayNotFound)?;
    owned_entry(txn, user_id, run.entry_id)
        .await?
        .ok_or(LibraryError::PlayNotFound)?;
    Ok((play, run.entry_id))
}

/// The entry with `entry_id` if it belongs to `user_id`.
async fn owned_entry(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry_id: Uuid,
) -> Result<Option<library_entry::Model>, LibraryError> {
    Ok(library_entry::Entity::find_by_id(entry_id)
        .one(txn)
        .await?
        .filter(|e| e.user_id == user_id))
}

/// The user's live entry for a title, creating it (status `initial`) or
/// restoring it from the trash as needed (writing the matching event), and
/// whether this transaction made it live: not when it was, or a race won.
async fn ensure_entry(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    media_id: Uuid,
    initial: EntryStatus,
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
            status_side_effects(txn, user_id, &entry).await?;
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

async fn restore_entry(
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
/// side effects.
async fn apply_patch(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry: library_entry::Model,
    patch: &EntryPatch,
) -> Result<library_entry::Model, LibraryError> {
    let Some(am) = patch.to_update(&entry) else {
        return Ok(entry);
    };
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
    if after.status != entry.status {
        status_side_effects(txn, user_id, &after).await?;
    }
    Ok(after)
}

async fn set_status(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry: library_entry::Model,
    status: EntryStatus,
) -> Result<library_entry::Model, LibraryError> {
    let patch = EntryPatch {
        status: Some(status),
        ..Default::default()
    };
    apply_patch(txn, user_id, entry, &patch).await
}

/// Keep runs consistent with a status the entry just moved to:
///
/// - `watching` opens run 1 (started today) when there is no run yet;
/// - `completed` stamps today as the current run's completion date if it has
///   none, opening run 1 first when needed.
///
/// Existing dates are never overwritten.
async fn status_side_effects(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    entry: &library_entry::Model,
) -> Result<(), LibraryError> {
    match entry.status {
        EntryStatus::Watching => {
            if repo::current_run(txn, entry.id).await?.is_none() {
                insert_run(txn, user_id, entry.id, 1, Some(today()), None).await?;
            }
        }
        EntryStatus::Completed => match repo::current_run(txn, entry.id).await? {
            None => {
                insert_run(txn, user_id, entry.id, 1, None, Some(today())).await?;
            }
            Some(run) if run.completed_on.is_none() => {
                let patch = RunPatch {
                    completed: crate::Patch::Value(today()),
                    ..Default::default()
                };
                if let Some(am) = patch.to_update(&run) {
                    let after = repo::update_run(txn, am).await?;
                    events::record(
                        txn,
                        entry.id,
                        user_id,
                        EntryEventKind::RunUpdated,
                        Some(&run),
                        Some(&after),
                    )
                    .await?;
                }
            }
            Some(_) => {}
        },
        EntryStatus::Planning | EntryStatus::Paused | EntryStatus::Dropped => {}
    }
    Ok(())
}

async fn insert_run(
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
