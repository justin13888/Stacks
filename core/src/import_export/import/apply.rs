//! Applying one matched import row to the library.
//!
//! Each row is one transaction that writes its `entry_events` like any
//! other library change. The row's facts become:
//!
//! - the entry's status, score, notes and favourite flag;
//! - run 1, carrying the row's start and finish dates at the precision the
//!   source gave them, and one further run per rewatch (MAL's "times
//!   rewatched", plus one when a rewatch is under way);
//! - plays in the current run, `source = import` and linked to the import:
//!   one per watched episode (episode 1 to the count, in season 1 — AniList
//!   numbers episodes absolutely), or one for a watched movie.
//!
//! Nothing is invented: a date the row lacks stays empty rather than
//! becoming today, and nothing the library holds is cleared. Under
//! [`ConflictStrategy::Merge`] a row only fills what the entry lacks; under
//! [`ConflictStrategy::Overwrite`] the row's values replace the entry's; an
//! existing entry under [`ConflictStrategy::Skip`] is left alone. Plays are
//! only ever added: an episode already watched in the current run is not
//! logged twice, so importing the same file again adds nothing.

use std::collections::{HashMap, HashSet};

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::enums::{ConflictStrategy, EntryEventKind, EntryStatus, MediaKind, PlaySource};
use crate::entity::{entry_event, entry_tag, library_entry, media, play, watch_run};
use crate::import_export::models::{ImportDate, ImportEntry};
use crate::library::{EntryPatch, RunDate, RunPatch, events, plays, repo};
use crate::media::catalog;
use crate::patch::Patch;

/// The most episodes one row may log, so a corrupt count cannot write
/// millions of plays.
pub const MAX_EPISODES: i32 = 10_000;

/// What applying a row did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Applied,
    /// An existing entry left alone by [`ConflictStrategy::Skip`].
    Skipped,
}

/// Apply `entry` to `user_id`'s library entry for `media` under `strategy`,
/// recording plays against `import_id`. On a transaction, the row is a
/// savepoint inside it: a row that fails leaves the caller's transaction
/// usable.
pub async fn apply<C: TransactionTrait<Transaction = DatabaseTransaction>>(
    db: &C,
    user_id: Uuid,
    media: &media::Model,
    entry: &ImportEntry,
    strategy: ConflictStrategy,
    import_id: Uuid,
) -> Result<Applied, String> {
    let txn = db.begin().await.map_err(|e| e.to_string())?;
    let applied = Row {
        txn: &txn,
        user_id,
        media,
        entry,
        import_id,
    }
    .apply(strategy)
    .await;
    match applied {
        Ok(applied) => {
            txn.commit().await.map_err(|e| e.to_string())?;
            Ok(applied)
        }
        Err(e) => {
            txn.rollback().await.map_err(|e| e.to_string())?;
            Err(e.to_string())
        }
    }
}

/// What taking a row's application back did to the old title's entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unapplied {
    /// The row had brought the entry into the library, and nothing but the
    /// row's own effects was on it: it went back to the trash.
    Trashed,
    /// The entry stays in the library with only the row's own plays voided,
    /// because it was there before the row, or because it carries something
    /// else: a live play the user or another row logged, a change made
    /// after the row was applied, or a tag.
    Kept,
    /// There is no live entry for the old title (the user trashed it
    /// already); the row's plays on it are voided all the same.
    Absent,
}

/// Take back what one import row's application did to `user_id`'s entry
/// for `media_id`, when the row is re-pointed at another title. The plays
/// it logged are voided, and no other play. An entry it created, or brought
/// back from the trash, goes to the trash only when nothing else is on it
/// ([`Unapplied::Trashed`]); otherwise it stays ([`Unapplied::Kept`]).
/// Changes the row made to an entry that was already in the library (its
/// status, score, notes, dates, extra runs) stay, each in the entry's
/// history. Nothing is deleted, so all of it can be undone.
///
/// The application is recognised by its events. A row's outcome is written
/// in the transaction that applied it ([`super::pipeline`]), and Postgres's
/// `now()` is that transaction's start, so the events the application
/// recorded carry exactly `applied_at`, the row's `updated_at` then: no
/// other transaction's events can. An event after `applied_at` is
/// something that happened to the entry since.
pub async fn unapply(
    txn: &DatabaseTransaction,
    user_id: Uuid,
    media_id: Uuid,
    import_id: Uuid,
    applied_at: DateTimeWithTimeZone,
) -> Result<Unapplied, sea_orm::DbErr> {
    let Some(entry) = repo::lock_entry(txn, user_id, media_id).await? else {
        return Ok(Unapplied::Absent);
    };
    let recorded = entry_event::Entity::find()
        .filter(entry_event::Column::EntryId.eq(entry.id))
        .filter(entry_event::Column::CreatedAt.eq(applied_at))
        .all(txn)
        .await?;
    // Read before this transaction records its own events below.
    let changed_since = entry_event::Entity::find()
        .filter(entry_event::Column::EntryId.eq(entry.id))
        .filter(entry_event::Column::CreatedAt.gt(applied_at))
        .one(txn)
        .await?
        .is_some();
    let logged = recorded
        .iter()
        .filter(|e| e.kind == EntryEventKind::PlayLogged)
        .filter_map(|e| e.after.as_ref()?.get("id")?.as_str()?.parse::<Uuid>().ok());
    for play_id in logged {
        let Some(play) = plays::lock_play(txn, play_id).await? else {
            continue;
        };
        if play.voided_at.is_some() || play.import_id != Some(import_id) {
            continue;
        }
        let after = plays::void_play(txn, play.id).await?;
        events::record(
            txn,
            entry.id,
            user_id,
            EntryEventKind::PlayVoided,
            Some(&play),
            Some(&after),
        )
        .await?;
    }
    if entry.deleted_at.is_some() {
        return Ok(Unapplied::Absent);
    }
    let brought_in = recorded.iter().any(|e| {
        matches!(
            e.kind,
            EntryEventKind::EntryCreated | EntryEventKind::EntryRestored
        )
    });
    if !brought_in || changed_since || carries_more(txn, entry.id).await? {
        return Ok(Unapplied::Kept);
    }
    let after = repo::set_deleted(txn, entry.id, true).await?;
    events::record(
        txn,
        entry.id,
        user_id,
        EntryEventKind::EntryDeleted,
        Some(&entry),
        Some(&after),
    )
    .await?;
    Ok(Unapplied::Trashed)
}

/// Whether an entry still holds a live play or a tag: something the row
/// being taken back did not put there, since its own plays are voided by
/// now and an import assigns no tags.
async fn carries_more(txn: &DatabaseTransaction, entry_id: Uuid) -> Result<bool, sea_orm::DbErr> {
    let runs: Vec<Uuid> = repo::runs_for_entry(txn, entry_id)
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect();
    if !plays::live_plays_in_runs(txn, &runs).await?.is_empty() {
        return Ok(true);
    }
    Ok(entry_tag::Entity::find()
        .filter(entry_tag::Column::EntryId.eq(entry_id))
        .one(txn)
        .await?
        .is_some())
}

struct Row<'a> {
    txn: &'a DatabaseTransaction,
    user_id: Uuid,
    media: &'a media::Model,
    entry: &'a ImportEntry,
    import_id: Uuid,
}

fn run_date(d: Option<ImportDate>) -> Option<RunDate> {
    d.map(|d| RunDate {
        date: d.date,
        precision: d.precision,
    })
}

impl Row<'_> {
    fn status(&self) -> Option<EntryStatus> {
        self.entry.status.as_deref().and_then(EntryStatus::parse)
    }

    fn score(&self) -> Option<i16> {
        self.entry
            .score
            .filter(|s| (0..=100).contains(s))
            .and_then(|s| i16::try_from(s).ok())
    }

    fn notes(&self) -> Option<String> {
        self.entry.notes.clone()
    }

    fn watched(&self) -> i32 {
        self.entry.progress.unwrap_or(0).clamp(0, MAX_EPISODES)
    }

    /// Whether the row says the title was watched at all, which is what
    /// opens a run.
    fn watched_at_all(&self) -> bool {
        self.watched() > 0
            || self.entry.started.is_some()
            || self.entry.completed.is_some()
            || self.entry.rewatch_count.unwrap_or(0) > 0
            || !matches!(self.status(), None | Some(EntryStatus::Planning))
    }

    async fn apply(&self, strategy: ConflictStrategy) -> Result<Applied, sea_orm::DbErr> {
        let existing = repo::lock_entry(self.txn, self.user_id, self.media.id).await?;
        let entry = match existing {
            None => self.create().await?,
            Some(e) if e.deleted_at.is_some() => {
                // A trashed entry comes back, and then takes the row as an
                // entry new to the library would: by merge at least.
                let restored = repo::set_deleted(self.txn, e.id, false).await?;
                events::record(
                    self.txn,
                    e.id,
                    self.user_id,
                    EntryEventKind::EntryRestored,
                    Some(&e),
                    Some(&restored),
                )
                .await?;
                let strategy = match strategy {
                    ConflictStrategy::Skip => ConflictStrategy::Merge,
                    s => s,
                };
                self.update(restored, strategy).await?
            }
            Some(_) if strategy == ConflictStrategy::Skip => return Ok(Applied::Skipped),
            Some(e) => self.update(e, strategy).await?,
        };
        self.runs(&entry, strategy).await?;
        self.plays(&entry).await?;
        Ok(Applied::Applied)
    }

    async fn create(&self) -> Result<library_entry::Model, sea_orm::DbErr> {
        let status = self.status().unwrap_or(if self.watched() > 0 {
            EntryStatus::Watching
        } else {
            EntryStatus::Planning
        });
        let entry = library_entry::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(self.user_id),
            media_id: Set(self.media.id),
            status: Set(status),
            score: Set(self.score()),
            favourite: Set(self.entry.favourite.unwrap_or(false)),
            notes: Set(self.notes()),
            deleted_at: Set(None),
            created_at: NotSet,
            updated_at: NotSet,
        }
        .insert(self.txn)
        .await?;
        events::record(
            self.txn,
            entry.id,
            self.user_id,
            EntryEventKind::EntryCreated,
            None::<&()>,
            Some(&entry),
        )
        .await?;
        Ok(entry)
    }

    async fn update(
        &self,
        current: library_entry::Model,
        strategy: ConflictStrategy,
    ) -> Result<library_entry::Model, sea_orm::DbErr> {
        let fill = |local: bool, value: Patch<i16>| if local { Patch::Absent } else { value };
        let patch = match strategy {
            ConflictStrategy::Overwrite => EntryPatch {
                status: self.status(),
                score: self.score().map_or(Patch::Absent, Patch::Value),
                favourite: self.entry.favourite,
                notes: self.notes().map_or(Patch::Absent, Patch::Value),
            },
            _ => EntryPatch {
                status: None,
                score: fill(
                    current.score.is_some(),
                    self.score().map_or(Patch::Absent, Patch::Value),
                ),
                favourite: self.entry.favourite.filter(|f| *f && !current.favourite),
                notes: match (&current.notes, self.notes()) {
                    (None, Some(n)) => Patch::Value(n),
                    _ => Patch::Absent,
                },
            },
        };
        let Some(am) = patch.to_update(&current) else {
            return Ok(current);
        };
        let after = am.update(self.txn).await?;
        events::record(
            self.txn,
            current.id,
            self.user_id,
            EntryEventKind::EntryUpdated,
            Some(&current),
            Some(&after),
        )
        .await?;
        Ok(after)
    }

    /// Run 1 with the row's dates, and a run per rewatch.
    async fn runs(
        &self,
        entry: &library_entry::Model,
        strategy: ConflictStrategy,
    ) -> Result<(), sea_orm::DbErr> {
        let mut runs = repo::runs_for_entry(self.txn, entry.id).await?;
        if runs.is_empty() && !self.watched_at_all() {
            return Ok(());
        }
        let started = run_date(self.entry.started);
        let completed = run_date(self.entry.completed);
        match runs.first() {
            None => {
                runs.push(self.insert_run(entry.id, 1, started, completed).await?);
            }
            Some(first) => {
                let set = |local: bool, value: Option<RunDate>| match (strategy, value) {
                    (_, None) => Patch::Absent,
                    (ConflictStrategy::Overwrite, Some(v)) => Patch::Value(v),
                    (_, Some(v)) if !local => Patch::Value(v),
                    _ => Patch::Absent,
                };
                let patch = RunPatch {
                    started: set(first.started_on.is_some(), started),
                    completed: set(first.completed_on.is_some(), completed),
                };
                if let Some(am) = patch.to_update(first) {
                    let after = am.update(self.txn).await?;
                    events::record(
                        self.txn,
                        entry.id,
                        self.user_id,
                        EntryEventKind::RunUpdated,
                        Some(first),
                        Some(&after),
                    )
                    .await?;
                }
            }
        }
        let wanted = 1
            + self.entry.rewatch_count.unwrap_or(0).clamp(0, 1000)
            + i32::from(self.entry.rewatching);
        let mut next = runs.iter().map(|r| r.run_number).max().unwrap_or(0) + 1;
        while next <= wanted {
            self.insert_run(entry.id, next, None, None).await?;
            next += 1;
        }
        Ok(())
    }

    async fn insert_run(
        &self,
        entry_id: Uuid,
        number: i32,
        started: Option<RunDate>,
        completed: Option<RunDate>,
    ) -> Result<watch_run::Model, sea_orm::DbErr> {
        let run = repo::insert_run(
            self.txn,
            watch_run::ActiveModel {
                id: Set(Uuid::now_v7()),
                entry_id: Set(entry_id),
                run_number: Set(number),
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
            self.txn,
            entry_id,
            self.user_id,
            EntryEventKind::RunStarted,
            None::<&()>,
            Some(&run),
        )
        .await?;
        Ok(run)
    }

    /// The watched episodes (or the watched movie) as plays in the current
    /// run, skipping what it already has.
    async fn plays(&self, entry: &library_entry::Model) -> Result<(), sea_orm::DbErr> {
        let Some(run) = repo::current_run(self.txn, entry.id).await? else {
            return Ok(());
        };
        let live = plays::live_plays_in_runs(self.txn, &[run.id]).await?;
        let mut episodes: Vec<Option<Uuid>> = Vec::new();
        match self.media.kind {
            MediaKind::Movie => {
                let seen = self.watched() > 0 || self.status() == Some(EntryStatus::Completed);
                if seen && live.is_empty() {
                    episodes.push(None);
                }
            }
            MediaKind::Show => {
                let have: HashSet<Uuid> = live.iter().filter_map(|p| p.episode_id).collect();
                let known: HashMap<i32, Uuid> =
                    catalog::episodes_of_many(self.txn, &[self.media.id])
                        .await?
                        .remove(&self.media.id)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|(season, _)| *season == 1)
                        .map(|(_, e)| (e.number, e.id))
                        .collect();
                for n in 1..=self.watched() {
                    let id = match known.get(&n) {
                        Some(id) => *id,
                        None => {
                            catalog::ensure_episode(self.txn, self.media.id, 1, n)
                                .await?
                                .id
                        }
                    };
                    if !have.contains(&id) {
                        episodes.push(Some(id));
                    }
                }
            }
        }
        if episodes.is_empty() {
            return Ok(());
        }
        // A movie watched on a known day is dated; an episode's day is not
        // known, so it is not invented.
        let watched_at = match self.media.kind {
            MediaKind::Movie => self
                .entry
                .completed
                .filter(|d| d.precision == crate::entity::enums::DatePrecision::Day)
                .and_then(|d| d.date.and_hms_opt(0, 0, 0))
                .map(|t| t.and_utc().into()),
            MediaKind::Show => None,
        };
        let now = crate::db::now();
        let logged: Vec<play::Model> = episodes
            .into_iter()
            .map(|episode_id| play::Model {
                id: Uuid::now_v7(),
                run_id: run.id,
                episode_id,
                watched_at,
                source: PlaySource::Import,
                import_id: Some(self.import_id),
                voided_at: None,
                created_at: now,
                updated_at: now,
            })
            .collect();
        plays::insert_plays(self.txn, &logged).await?;
        let afters: Vec<(Uuid, &play::Model)> = logged.iter().map(|p| (entry.id, p)).collect();
        events::record_many(self.txn, self.user_id, EntryEventKind::PlayLogged, &afters).await
    }
}
