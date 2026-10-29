//! Restoring a [`Backup`] into an account.
//!
//! A restore is one transaction: it applies everything or nothing, and a dry
//! run does all of the work and rolls it back, so its report is exactly what
//! the real run would do.
//!
//! Rows keep their ids unless the id is already taken by a row that is not
//! the same one (restoring a backup into a second account on the instance it
//! was taken from), in which case the row gets a fresh id and every
//! reference to it, including the ids inside event payloads, follows.
//!
//! What an existing row means under each [`RestoreMode`]:
//!
//! - **merge** (the default): an incoming value fills a field that is absent
//!   locally (a missing score, notes, a run date, a tag colour); a field set
//!   on both sides keeps the local value. Plays, events and tag assignments
//!   are unioned by id, and a play the backup has voided is voided here too,
//!   unless this account restored it after that void.
//! - **skip**: an existing entry, list, tag or channel is left untouched,
//!   with everything under it; only what is missing is added.
//! - **overwrite**: the incoming values replace the local ones. Nothing is
//!   ever deleted or un-voided: plays and events are still unioned.
//!
//! Every change to a row that already existed is written with its
//! `entry_events` row, like any other library mutation; rows the restore
//! inserts bring their own history from the backup.

use std::collections::{HashMap, HashSet};

use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    PrimaryKeyTrait, QueryFilter, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::entity::enums::{EntryEventKind, RefProvider};
use crate::entity::{
    blocklist, entry_event, entry_tag, import, library_entry, list, list_item, media_ref,
    notification_channel, notification_subscription, play, tag, user, watch_run,
};
use crate::library::events;
use crate::lists::fractional_index;
use crate::media::catalog;
use crate::media::{MediaId, Provider};
use crate::notifications::service::{valid_name, validate_config};

use super::format::{self, Backup};
use super::upgrade::{self, ReadError};

mod event_log;
mod filter;
mod validate;

use filter::canonical_filter;
use validate::validate;

/// What a restore does to rows that already exist.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    /// Fill in only what is missing locally; union plays and events.
    #[default]
    Merge,
    /// Leave existing rows untouched; add what is missing.
    Skip,
    /// Let the backup's values win; union plays and events.
    Overwrite,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RestoreOptions {
    pub mode: RestoreMode,
    /// Do everything, report it, and keep none of it.
    pub dry_run: bool,
}

/// How the rows of one kind fared.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    /// Added.
    pub inserted: u32,
    /// Existed, and changed.
    pub updated: u32,
    /// Existed, and already held what the backup holds.
    pub unchanged: u32,
    /// Left alone: by `skip`, or because they could not be restored (see the
    /// report's warnings).
    pub skipped: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleTally {
    /// Already in this catalog.
    pub matched: u32,
    /// Added to the catalog, to be fetched by the refresh job.
    pub created: u32,
}

/// What happened to one library entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Inserted,
    Updated,
    Unchanged,
    Skipped,
}

/// One library entry's outcome, for the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryChange {
    /// The title's public id (`tmdb:movie:550`).
    pub title: String,
    pub action: Action,
}

/// What a restore did, or with `dry_run`, would do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreReport {
    /// The format version the backup was written in.
    pub read_version: u32,
    pub mode: RestoreMode,
    pub dry_run: bool,
    pub settings_changed: bool,
    pub titles: TitleTally,
    pub entries: Tally,
    pub runs: Tally,
    pub plays: Tally,
    pub events: Tally,
    pub entry_tags: Tally,
    pub lists: Tally,
    pub list_items: Tally,
    pub tags: Tally,
    pub blocklist: Tally,
    pub notification_channels: Tally,
    pub notification_subscriptions: Tally,
    /// Each library entry, in backup order.
    pub changes: Vec<EntryChange>,
    /// What could not be restored as it was, and why.
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error(transparent)]
    Read(#[from] ReadError),
    /// The document reads, but contradicts itself (a reference to a title it
    /// does not hold, a title without a primary id, settings no release
    /// would write).
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Db(#[from] DbErr),
}

/// Restore a backup file into `user_id`'s account.
pub async fn restore(
    db: &DatabaseConnection,
    user_id: Uuid,
    bytes: &[u8],
    options: RestoreOptions,
) -> Result<RestoreReport, RestoreError> {
    let read = upgrade::read(bytes)?;
    validate(&read.backup)?;
    let txn = db.begin().await?;
    let mut run = Restorer {
        txn: &txn,
        user_id,
        mode: options.mode,
        ids: HashMap::new(),
        claimed_events: HashSet::new(),
        report: RestoreReport {
            read_version: read.read_version,
            mode: options.mode,
            dry_run: options.dry_run,
            ..Default::default()
        },
    };
    run.restore(&read.backup).await?;
    let report = run.report;
    if options.dry_run {
        txn.rollback().await?;
    } else {
        txn.commit().await?;
    }
    Ok(report)
}

struct Restorer<'t> {
    txn: &'t DatabaseTransaction,
    user_id: Uuid,
    mode: RestoreMode,
    /// Backup id → local id, for every row restored or matched.
    ids: HashMap<Uuid, Uuid>,
    /// Local events this restore inserted or matched to a backup event.
    claimed_events: HashSet<Uuid>,
    report: RestoreReport,
}

/// The backup's id when no row of `E` has it, else a fresh one.
async fn free_id<E>(txn: &DatabaseTransaction, id: Uuid) -> Result<Uuid, DbErr>
where
    E: EntityTrait,
    <E::PrimaryKey as PrimaryKeyTrait>::ValueType: From<Uuid>,
{
    Ok(if E::find_by_id(id).one(txn).await?.is_some() {
        Uuid::now_v7()
    } else {
        id
    })
}

/// Rewrite every id member (`id`, `*_id`) of an event payload the restore
/// gave a new id.
fn remap(value: &mut Value, ids: &HashMap<Uuid, Uuid>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if (key == "id" || key.ends_with("_id"))
                    && let Value::String(s) = v
                    && let Ok(old) = Uuid::parse_str(s)
                {
                    if let Some(new) = ids.get(&old) {
                        *s = new.to_string();
                    }
                } else {
                    remap(v, ids);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| remap(v, ids)),
        _ => {}
    }
}

/// `local`, or with `merge` the incoming value where `local` is absent, or
/// with `overwrite` the incoming value.
fn pick<T: Clone>(mode: RestoreMode, local: &Option<T>, incoming: &Option<T>) -> Option<T> {
    match mode {
        RestoreMode::Overwrite => incoming.clone(),
        RestoreMode::Merge => local.clone().or_else(|| incoming.clone()),
        RestoreMode::Skip => local.clone(),
    }
}

impl Restorer<'_> {
    fn warn(&mut self, message: String) {
        self.report.warnings.push(message);
    }

    async fn restore(&mut self, b: &Backup) -> Result<(), RestoreError> {
        if let Some(profile) = &b.user {
            self.ids.insert(profile.id, self.user_id);
        }
        self.settings(&b.settings).await?;
        let mut public: HashMap<Uuid, String> = HashMap::new();
        for t in &b.titles {
            let local = self.title(t).await?;
            self.ids.insert(t.id, local);
            if let Some(p) = t.refs.iter().find(|r| r.primary) {
                public.insert(t.id, format!("{}:{}:{}", p.provider, p.kind, p.external_id));
            }
        }
        for t in &b.tags {
            self.tag(t).await?;
        }
        for e in &b.entries {
            let action = self.entry(e).await?;
            self.report.changes.push(EntryChange {
                title: public.get(&e.title_id).cloned().unwrap_or_default(),
                action,
            });
        }
        for l in &b.lists {
            self.list(l).await?;
        }
        for x in &b.blocklist {
            self.blocked(x).await?;
        }
        for c in &b.notification_channels {
            self.channel(c).await?;
        }
        Ok(())
    }

    async fn settings(
        &mut self,
        incoming: &serde_json::Map<String, Value>,
    ) -> Result<(), RestoreError> {
        let owner = user::Entity::find_by_id(self.user_id)
            .one(self.txn)
            .await?
            .ok_or_else(|| RestoreError::Invalid(format!("user {} not found", self.user_id)))?;
        let Value::Object(mut settings) = owner.settings.clone() else {
            return Ok(());
        };
        for (k, v) in incoming {
            match self.mode {
                RestoreMode::Skip => {}
                RestoreMode::Merge => {
                    settings.entry(k.clone()).or_insert_with(|| v.clone());
                }
                RestoreMode::Overwrite => {
                    settings.insert(k.clone(), v.clone());
                }
            }
        }
        let settings = Value::Object(settings);
        if settings != owner.settings {
            user::ActiveModel {
                id: Unchanged(owner.id),
                settings: Set(settings),
                ..Default::default()
            }
            .update(self.txn)
            .await?;
            self.report.settings_changed = true;
        }
        Ok(())
    }

    /// The local catalog row for a title: one any of its ids already names,
    /// or a new never-fetched one.
    async fn title(&mut self, t: &format::Title) -> Result<Uuid, RestoreError> {
        let mut found = None;
        for r in t.refs.iter().filter(|r| r.primary).chain(t.refs.iter()) {
            if let Some(row) = find_ref(self.txn, r.provider, r.kind, &r.external_id).await? {
                found = Some(row.media_id);
                break;
            }
        }
        let media_id = match found {
            Some(id) => {
                self.report.titles.matched += 1;
                id
            }
            None => {
                let primary = t.refs.iter().find(|r| r.primary).ok_or_else(|| {
                    RestoreError::Invalid(format!("title {} has no primary id", t.id))
                })?;
                let provider = Provider::from_ref_provider(primary.provider).ok_or_else(|| {
                    RestoreError::Invalid(format!("title {}'s primary id is not fetchable", t.id))
                })?;
                let numeric = primary.external_id.parse::<i32>().map_err(|_| {
                    RestoreError::Invalid(format!("title {}'s primary id is not numeric", t.id))
                })?;
                let id = MediaId::new(provider, primary.kind, numeric);
                let row_id = free_id::<crate::entity::media::Entity>(self.txn, t.id).await?;
                catalog::insert_unfetched(self.txn, row_id, &id, &t.name).await?;
                self.report.titles.created += 1;
                row_id
            }
        };
        // The catalog's links are shared by every account, so a backup never
        // writes one: a title's other ids come from its provider when the
        // refresh job fetches it. An id that names another title here is
        // reported.
        for r in &t.refs {
            if let Some(existing) = find_ref(self.txn, r.provider, r.kind, &r.external_id).await?
                && existing.media_id != media_id
            {
                self.warn(format!(
                    "{}:{}:{} already names another title here; it was not linked to {}",
                    r.provider, r.kind, r.external_id, t.name
                ));
            }
        }
        Ok(media_id)
    }

    async fn tag(&mut self, t: &format::Tag) -> Result<(), RestoreError> {
        let by_id = tag::Entity::find_by_id(t.id)
            .one(self.txn)
            .await?
            .filter(|x| x.user_id == self.user_id);
        let local = match by_id {
            Some(x) => Some(x),
            None => {
                tag::Entity::find()
                    .filter(tag::Column::UserId.eq(self.user_id))
                    .filter(tag::Column::Name.eq(t.name.clone()))
                    .one(self.txn)
                    .await?
            }
        };
        let Some(local) = local else {
            let id = free_id::<tag::Entity>(self.txn, t.id).await?;
            tag::ActiveModel {
                id: Set(id),
                user_id: Set(self.user_id),
                name: Set(t.name.clone()),
                color: Set(t.color.clone()),
                created_at: Set(t.created_at),
                updated_at: Set(t.updated_at),
            }
            .insert(self.txn)
            .await?;
            self.ids.insert(t.id, id);
            self.report.tags.inserted += 1;
            return Ok(());
        };
        self.ids.insert(t.id, local.id);
        if self.mode == RestoreMode::Skip {
            self.report.tags.skipped += 1;
            return Ok(());
        }
        let color = pick(self.mode, &local.color, &t.color);
        let name = match self.mode {
            RestoreMode::Overwrite => t.name.clone(),
            _ => local.name.clone(),
        };
        if color == local.color && name == local.name {
            self.report.tags.unchanged += 1;
            return Ok(());
        }
        let clash = name != local.name
            && tag::Entity::find()
                .filter(tag::Column::UserId.eq(self.user_id))
                .filter(tag::Column::Name.eq(name.clone()))
                .one(self.txn)
                .await?
                .is_some();
        tag::ActiveModel {
            id: Unchanged(local.id),
            name: if clash { NotSet } else { Set(name) },
            color: Set(color),
            ..Default::default()
        }
        .update(self.txn)
        .await?;
        self.report.tags.updated += 1;
        Ok(())
    }

    async fn entry(&mut self, e: &format::Entry) -> Result<Action, RestoreError> {
        let media_id = self.local(e.title_id)?;
        let existing = library_entry::Entity::find()
            .filter(library_entry::Column::UserId.eq(self.user_id))
            .filter(library_entry::Column::MediaId.eq(media_id))
            .one(self.txn)
            .await?;
        let Some(local) = existing else {
            self.insert_entry(e, media_id).await?;
            self.report.entries.inserted += 1;
            return Ok(Action::Inserted);
        };
        self.ids.insert(e.id, local.id);
        if self.mode == RestoreMode::Skip {
            self.report.entries.skipped += 1;
            return Ok(Action::Skipped);
        }
        let mut changed = self.update_entry(e, &local).await?;
        for r in &e.runs {
            changed |= self.merge_run(r, &local, media_id).await?;
        }
        for ev in &e.events {
            self.union_event(ev, local.id).await?;
        }
        for t in &e.tags {
            changed |= self.union_entry_tag(t, local.id).await?;
        }
        Ok(if changed {
            self.report.entries.updated += 1;
            Action::Updated
        } else {
            self.report.entries.unchanged += 1;
            Action::Unchanged
        })
    }

    fn local(&self, backup_id: Uuid) -> Result<Uuid, RestoreError> {
        self.ids
            .get(&backup_id)
            .copied()
            .ok_or_else(|| RestoreError::Invalid(format!("{backup_id} names nothing restored")))
    }

    async fn insert_entry(
        &mut self,
        e: &format::Entry,
        media_id: Uuid,
    ) -> Result<(), RestoreError> {
        let id = free_id::<library_entry::Entity>(self.txn, e.id).await?;
        self.ids.insert(e.id, id);
        library_entry::ActiveModel {
            id: Set(id),
            user_id: Set(self.user_id),
            media_id: Set(media_id),
            status: Set(e.status),
            score: Set(e.score),
            favourite: Set(e.favourite),
            notes: Set(e.notes.clone()),
            deleted_at: Set(e.deleted_at),
            created_at: Set(e.created_at),
            updated_at: Set(e.updated_at),
        }
        .insert(self.txn)
        .await?;
        for r in &e.runs {
            self.insert_run(r, id, media_id).await?;
        }
        for ev in &e.events {
            self.union_event(ev, id).await?;
        }
        for t in &e.tags {
            self.union_entry_tag(t, id).await?;
        }
        Ok(())
    }

    /// Apply the mode to an existing entry's own fields, with the events
    /// that describe the change. Returns whether anything changed.
    async fn update_entry(
        &mut self,
        e: &format::Entry,
        local: &library_entry::Model,
    ) -> Result<bool, RestoreError> {
        let (status, favourite, deleted_at) = match self.mode {
            RestoreMode::Overwrite => (e.status, e.favourite, e.deleted_at),
            _ => (
                local.status,
                local.favourite || e.favourite,
                local.deleted_at,
            ),
        };
        let score = pick(self.mode, &local.score, &e.score);
        let notes = pick(self.mode, &local.notes, &e.notes);
        let mut current = local.clone();
        let fields_change = status != local.status
            || favourite != local.favourite
            || score != local.score
            || notes != local.notes;
        if fields_change {
            let after = library_entry::ActiveModel {
                id: Unchanged(local.id),
                status: Set(status),
                favourite: Set(favourite),
                score: Set(score),
                notes: Set(notes),
                ..Default::default()
            }
            .update(self.txn)
            .await?;
            events::record(
                self.txn,
                local.id,
                self.user_id,
                EntryEventKind::EntryUpdated,
                Some(&current),
                Some(&after),
            )
            .await?;
            current = after;
        }
        let trash_change = deleted_at.is_some() != local.deleted_at.is_some();
        if trash_change {
            let after = library_entry::ActiveModel {
                id: Unchanged(local.id),
                deleted_at: Set(deleted_at),
                ..Default::default()
            }
            .update(self.txn)
            .await?;
            let kind = if deleted_at.is_some() {
                EntryEventKind::EntryDeleted
            } else {
                EntryEventKind::EntryRestored
            };
            events::record(
                self.txn,
                local.id,
                self.user_id,
                kind,
                Some(&current),
                Some(&after),
            )
            .await?;
        }
        Ok(fields_change || trash_change)
    }

    async fn insert_run(
        &mut self,
        r: &format::Run,
        entry_id: Uuid,
        media_id: Uuid,
    ) -> Result<(), RestoreError> {
        let id = free_id::<watch_run::Entity>(self.txn, r.id).await?;
        self.ids.insert(r.id, id);
        watch_run::ActiveModel {
            id: Set(id),
            entry_id: Set(entry_id),
            run_number: Set(r.number),
            started_on: Set(r.started_on),
            started_precision: Set(r.started_precision),
            completed_on: Set(r.completed_on),
            completed_precision: Set(r.completed_precision),
            created_at: Set(r.created_at),
            updated_at: Set(r.updated_at),
        }
        .insert(self.txn)
        .await?;
        self.report.runs.inserted += 1;
        for p in &r.plays {
            self.insert_play(p, id, media_id).await?;
        }
        Ok(())
    }

    /// Match an incoming run to the local entry's run of the same id or
    /// number, apply the mode to its dates, and union its plays. Returns
    /// whether anything changed.
    async fn merge_run(
        &mut self,
        r: &format::Run,
        entry: &library_entry::Model,
        media_id: Uuid,
    ) -> Result<bool, RestoreError> {
        let by_id = watch_run::Entity::find_by_id(r.id)
            .one(self.txn)
            .await?
            .filter(|x| x.entry_id == entry.id);
        let local = match by_id {
            Some(x) => Some(x),
            None => {
                watch_run::Entity::find()
                    .filter(watch_run::Column::EntryId.eq(entry.id))
                    .filter(watch_run::Column::RunNumber.eq(r.number))
                    .one(self.txn)
                    .await?
            }
        };
        let Some(local) = local else {
            self.insert_run(r, entry.id, media_id).await?;
            return Ok(true);
        };
        self.ids.insert(r.id, local.id);
        let date = |d, p| match (d, p) {
            (Some(d), Some(p)) => Some((d, p)),
            _ => None,
        };
        let started = pick(
            self.mode,
            &date(local.started_on, local.started_precision),
            &date(r.started_on, r.started_precision),
        );
        let completed = pick(
            self.mode,
            &date(local.completed_on, local.completed_precision),
            &date(r.completed_on, r.completed_precision),
        );
        let mut changed = false;
        if started != date(local.started_on, local.started_precision)
            || completed != date(local.completed_on, local.completed_precision)
        {
            let after = watch_run::ActiveModel {
                id: Unchanged(local.id),
                started_on: Set(started.map(|d| d.0)),
                started_precision: Set(started.map(|d| d.1)),
                completed_on: Set(completed.map(|d| d.0)),
                completed_precision: Set(completed.map(|d| d.1)),
                ..Default::default()
            }
            .update(self.txn)
            .await?;
            events::record(
                self.txn,
                entry.id,
                self.user_id,
                EntryEventKind::RunUpdated,
                Some(&local),
                Some(&after),
            )
            .await?;
            self.report.runs.updated += 1;
            changed = true;
        } else {
            self.report.runs.unchanged += 1;
        }
        for p in &r.plays {
            changed |= self.union_play(p, local.id, media_id).await?;
        }
        Ok(changed)
    }

    async fn insert_play(
        &mut self,
        p: &format::Play,
        run_id: Uuid,
        media_id: Uuid,
    ) -> Result<(), RestoreError> {
        let id = free_id::<play::Entity>(self.txn, p.id).await?;
        self.ids.insert(p.id, id);
        let episode_id = match &p.episode {
            Some(ep) => {
                let local =
                    catalog::ensure_episode(self.txn, media_id, ep.season, ep.number).await?;
                self.ids.insert(ep.id, local.id);
                Some(local.id)
            }
            None => None,
        };
        // An import of another account (or another instance) is not this
        // account's: the play keeps its `import` source without the link.
        let import_id = match p.import_id {
            Some(i) => import::Entity::find_by_id(i)
                .one(self.txn)
                .await?
                .filter(|x| x.user_id == self.user_id)
                .map(|x| x.id),
            None => None,
        };
        play::ActiveModel {
            id: Set(id),
            run_id: Set(run_id),
            episode_id: Set(episode_id),
            watched_at: Set(p.watched_at),
            source: Set(p.source),
            import_id: Set(import_id),
            voided_at: Set(p.voided_at),
            created_at: Set(p.created_at),
            updated_at: Set(p.updated_at),
        }
        .insert(self.txn)
        .await?;
        self.report.plays.inserted += 1;
        Ok(())
    }

    /// Add a play the local run lacks; void a local play the backup has
    /// voided, unless the account restored it since. A restore never brings
    /// a voided play back: only the account's own `restore_play` does.
    async fn union_play(
        &mut self,
        p: &format::Play,
        run_id: Uuid,
        media_id: Uuid,
    ) -> Result<bool, RestoreError> {
        let by_id = play::Entity::find_by_id(p.id)
            .one(self.txn)
            .await?
            .filter(|x| x.run_id == run_id);
        let local = match by_id {
            Some(x) => Some(x),
            // A play an earlier restore gave a new id (its own was taken)
            // is still the same play: same run, episode and logging time.
            None => self.same_play(p, run_id, media_id).await?,
        };
        let Some(local) = local else {
            self.insert_play(p, run_id, media_id).await?;
            return Ok(true);
        };
        self.ids.insert(p.id, local.id);
        if let Some(ep) = &p.episode
            && let Some(local_ep) = local.episode_id
        {
            self.ids.insert(ep.id, local_ep);
        }
        if local.voided_at.is_none()
            && let Some(voided_at) = p.voided_at
            && !self.restored_since(local.id, voided_at).await?
        {
            play::ActiveModel {
                id: Unchanged(local.id),
                voided_at: Set(p.voided_at),
                ..Default::default()
            }
            .update(self.txn)
            .await?;
            self.report.plays.updated += 1;
            return Ok(true);
        }
        self.report.plays.unchanged += 1;
        Ok(false)
    }

    /// Whether the account restored the play after `voided_at`: then the
    /// backup's void is older than the account's own undo of it, and voiding
    /// the play again would throw that undo away.
    async fn restored_since(
        &self,
        play_id: Uuid,
        voided_at: sea_orm::prelude::DateTimeWithTimeZone,
    ) -> Result<bool, RestoreError> {
        use sea_orm::PaginatorTrait;
        let restorations = entry_event::Entity::find()
            .filter(entry_event::Column::UserId.eq(self.user_id))
            .filter(entry_event::Column::Kind.eq(EntryEventKind::PlayRestored))
            .filter(entry_event::Column::CreatedAt.gt(voided_at))
            .filter(sea_orm::sea_query::Expr::cust_with_values(
                "entry_events.after ->> 'id' = $1",
                [play_id.to_string()],
            ))
            .count(self.txn)
            .await?;
        Ok(restorations > 0)
    }

    /// The local play in `run_id` that is `p` under another id.
    async fn same_play(
        &self,
        p: &format::Play,
        run_id: Uuid,
        media_id: Uuid,
    ) -> Result<Option<play::Model>, RestoreError> {
        let episode_id = match &p.episode {
            Some(ep) => {
                match catalog::find_episode(self.txn, media_id, ep.season, ep.number).await? {
                    Some(local) => Some(local.id),
                    None => return Ok(None),
                }
            }
            None => None,
        };
        Ok(play::Entity::find()
            .filter(play::Column::RunId.eq(run_id))
            .filter(play::Column::CreatedAt.eq(p.created_at))
            .all(self.txn)
            .await?
            .into_iter()
            .find(|x| x.episode_id == episode_id && x.watched_at == p.watched_at))
    }

    async fn union_entry_tag(
        &mut self,
        t: &format::EntryTag,
        entry_id: Uuid,
    ) -> Result<bool, RestoreError> {
        let tag_id = self.local(t.tag_id)?;
        let exists = entry_tag::Entity::find()
            .filter(entry_tag::Column::EntryId.eq(entry_id))
            .filter(entry_tag::Column::TagId.eq(tag_id))
            .one(self.txn)
            .await?
            .is_some();
        if exists {
            self.report.entry_tags.unchanged += 1;
            return Ok(false);
        }
        let id = free_id::<entry_tag::Entity>(self.txn, t.id).await?;
        entry_tag::ActiveModel {
            id: Set(id),
            entry_id: Set(entry_id),
            tag_id: Set(tag_id),
            created_at: Set(t.created_at),
            updated_at: Set(t.updated_at),
        }
        .insert(self.txn)
        .await?;
        self.report.entry_tags.inserted += 1;
        Ok(true)
    }

    async fn list(&mut self, l: &format::List) -> Result<(), RestoreError> {
        let by_id = list::Entity::find_by_id(l.id)
            .one(self.txn)
            .await?
            .filter(|x| x.user_id == self.user_id);
        let local = match by_id {
            Some(x) => Some(x),
            // Restored earlier under a new id: same name, same creation.
            None => {
                list::Entity::find()
                    .filter(list::Column::UserId.eq(self.user_id))
                    .filter(list::Column::Name.eq(l.name.clone()))
                    .filter(list::Column::CreatedAt.eq(l.created_at))
                    .one(self.txn)
                    .await?
            }
        };
        let Some(local) = local else {
            let id = free_id::<list::Entity>(self.txn, l.id).await?;
            list::ActiveModel {
                id: Set(id),
                user_id: Set(self.user_id),
                name: Set(l.name.clone()),
                description: Set(l.description.clone()),
                is_smart: Set(l.smart),
                filter_query: Set(self.local_filter(l).await?),
                deleted_at: Set(l.deleted_at),
                created_at: Set(l.created_at),
                updated_at: Set(l.updated_at),
            }
            .insert(self.txn)
            .await?;
            self.report.lists.inserted += 1;
            for i in &l.items {
                self.insert_item(i, id, &i.position).await?;
            }
            return Ok(());
        };
        if self.mode == RestoreMode::Skip {
            self.report.lists.skipped += 1;
            self.report.list_items.skipped += l.items.len() as u32;
            return Ok(());
        }
        let (name, smart, deleted_at) = match self.mode {
            RestoreMode::Overwrite => (l.name.clone(), l.smart, l.deleted_at),
            _ => (local.name.clone(), local.is_smart, local.deleted_at),
        };
        let description = pick(self.mode, &local.description, &l.description);
        let local_filter = canonical_filter(&local.filter_query);
        let incoming = self.local_filter(l).await?;
        let filter = pick(self.mode, &local_filter, &incoming);
        if name != local.name
            || smart != local.is_smart
            || deleted_at != local.deleted_at
            || description != local.description
            || filter != local_filter
        {
            list::ActiveModel {
                id: Unchanged(local.id),
                name: Set(name),
                description: Set(description),
                is_smart: Set(smart),
                filter_query: Set(filter),
                deleted_at: Set(deleted_at),
                ..Default::default()
            }
            .update(self.txn)
            .await?;
            self.report.lists.updated += 1;
        } else {
            self.report.lists.unchanged += 1;
        }
        let items = list_item::Entity::find()
            .filter(list_item::Column::ListId.eq(local.id))
            .all(self.txn)
            .await?;
        let mut positions: HashSet<String> = items.iter().map(|i| i.position.clone()).collect();
        let by_media: HashMap<Uuid, list_item::Model> =
            items.into_iter().map(|i| (i.media_id, i)).collect();
        for i in &l.items {
            let media_id = self.local(i.title_id)?;
            if let Some(existing) = by_media.get(&media_id) {
                let notes = pick(self.mode, &existing.notes, &i.notes);
                if notes != existing.notes {
                    list_item::ActiveModel {
                        id: Unchanged(existing.id),
                        notes: Set(notes),
                        ..Default::default()
                    }
                    .update(self.txn)
                    .await?;
                    self.report.list_items.updated += 1;
                } else {
                    self.report.list_items.unchanged += 1;
                }
                continue;
            }
            // A position the local list already uses goes to the end.
            let position = if positions.contains(&i.position) {
                let last = positions.iter().max().cloned();
                fractional_index::between(last.as_deref(), None)
                    .map_err(|e| RestoreError::Invalid(format!("list item {}: {e}", i.id)))?
            } else {
                i.position.clone()
            };
            positions.insert(position.clone());
            self.insert_item(i, local.id, &position).await?;
        }
        Ok(())
    }

    async fn insert_item(
        &mut self,
        i: &format::ListItem,
        list_id: Uuid,
        position: &str,
    ) -> Result<(), RestoreError> {
        let id = free_id::<list_item::Entity>(self.txn, i.id).await?;
        list_item::ActiveModel {
            id: Set(id),
            list_id: Set(list_id),
            media_id: Set(self.local(i.title_id)?),
            position: Set(position.to_owned()),
            notes: Set(i.notes.clone()),
            created_at: Set(i.created_at),
            updated_at: Set(i.updated_at),
        }
        .insert(self.txn)
        .await?;
        self.report.list_items.inserted += 1;
        Ok(())
    }

    async fn blocked(&mut self, x: &format::Blocked) -> Result<(), RestoreError> {
        let media_id = self.local(x.title_id)?;
        let exists = blocklist::Entity::find()
            .filter(blocklist::Column::UserId.eq(self.user_id))
            .filter(blocklist::Column::MediaId.eq(media_id))
            .one(self.txn)
            .await?
            .is_some();
        if exists {
            self.report.blocklist.unchanged += 1;
            return Ok(());
        }
        let id = free_id::<blocklist::Entity>(self.txn, x.id).await?;
        blocklist::ActiveModel {
            id: Set(id),
            user_id: Set(self.user_id),
            media_id: Set(media_id),
            created_at: Set(x.created_at),
            updated_at: Set(x.updated_at),
        }
        .insert(self.txn)
        .await?;
        self.report.blocklist.inserted += 1;
        Ok(())
    }

    async fn channel(&mut self, c: &format::Channel) -> Result<(), RestoreError> {
        let Some(config) = c.config.clone().map(Value::Object) else {
            self.report.notification_channels.skipped += 1;
            self.report.notification_subscriptions.skipped += c.subscriptions.len() as u32;
            self.warn(format!(
                "notification channel {:?} was backed up without its configuration; \
                 create it again",
                c.name
            ));
            return Ok(());
        };
        let by_id = notification_channel::Entity::find_by_id(c.id)
            .one(self.txn)
            .await?
            .filter(|x| x.user_id == self.user_id);
        let local = match by_id {
            Some(x) => Some(x),
            // Restored earlier under a new id: same kind, name and creation.
            None => {
                notification_channel::Entity::find()
                    .filter(notification_channel::Column::UserId.eq(self.user_id))
                    .filter(notification_channel::Column::ChannelType.eq(c.channel_type))
                    .filter(notification_channel::Column::Name.eq(c.name.clone()))
                    .filter(notification_channel::Column::CreatedAt.eq(c.created_at))
                    .one(self.txn)
                    .await?
            }
        };
        // A channel the restore writes passes the channel API's own rules
        // (its URL, its headers, a non-blank name); one that does not is left
        // out, with its subscriptions, and reported.
        let checked =
            validate_config(c.channel_type, &config).and_then(|()| valid_name(c.name.clone()));
        let channel_id = match local {
            Some(local) => {
                let writes = self.mode == RestoreMode::Overwrite
                    && (local.name != c.name
                        || local.enabled != c.enabled
                        || local.config != config)
                    && local.channel_type == c.channel_type;
                if writes {
                    let name = match checked {
                        Ok(name) => name,
                        Err(e) => {
                            self.report.notification_channels.skipped += 1;
                            self.report.notification_subscriptions.skipped +=
                                c.subscriptions.len() as u32;
                            self.warn(format!(
                                "notification channel {:?} was not overwritten: {e}",
                                c.name
                            ));
                            return Ok(());
                        }
                    };
                    notification_channel::ActiveModel {
                        id: Unchanged(local.id),
                        name: Set(name),
                        enabled: Set(c.enabled),
                        config: Set(config),
                        ..Default::default()
                    }
                    .update(self.txn)
                    .await?;
                    self.report.notification_channels.updated += 1;
                } else if self.mode == RestoreMode::Skip {
                    self.report.notification_channels.skipped += 1;
                    self.report.notification_subscriptions.skipped += c.subscriptions.len() as u32;
                    return Ok(());
                } else {
                    self.report.notification_channels.unchanged += 1;
                }
                local.id
            }
            None => {
                let name = match checked {
                    Ok(name) => name,
                    Err(e) => {
                        self.report.notification_channels.skipped += 1;
                        self.report.notification_subscriptions.skipped +=
                            c.subscriptions.len() as u32;
                        self.warn(format!(
                            "notification channel {:?} was not restored: {e}",
                            c.name
                        ));
                        return Ok(());
                    }
                };
                let id = free_id::<notification_channel::Entity>(self.txn, c.id).await?;
                notification_channel::ActiveModel {
                    id: Set(id),
                    user_id: Set(self.user_id),
                    channel_type: Set(c.channel_type),
                    name: Set(name),
                    config: Set(config),
                    enabled: Set(c.enabled),
                    created_at: Set(c.created_at),
                    updated_at: Set(c.updated_at),
                }
                .insert(self.txn)
                .await?;
                self.report.notification_channels.inserted += 1;
                id
            }
        };
        for s in &c.subscriptions {
            self.subscription(s, channel_id).await?;
        }
        Ok(())
    }

    async fn subscription(
        &mut self,
        s: &format::Subscription,
        channel_id: Uuid,
    ) -> Result<(), RestoreError> {
        let incoming = s.config.clone().map(Value::Object);
        let local = notification_subscription::Entity::find()
            .filter(notification_subscription::Column::UserId.eq(self.user_id))
            .filter(notification_subscription::Column::ChannelId.eq(channel_id))
            .filter(notification_subscription::Column::EventType.eq(s.event_type))
            .one(self.txn)
            .await?;
        let Some(local) = local else {
            let id = free_id::<notification_subscription::Entity>(self.txn, s.id).await?;
            notification_subscription::ActiveModel {
                id: Set(id),
                user_id: Set(self.user_id),
                channel_id: Set(channel_id),
                event_type: Set(s.event_type),
                enabled: Set(s.enabled),
                config: Set(incoming),
                created_at: Set(s.created_at),
                updated_at: Set(s.updated_at),
            }
            .insert(self.txn)
            .await?;
            self.report.notification_subscriptions.inserted += 1;
            return Ok(());
        };
        if self.mode == RestoreMode::Skip {
            self.report.notification_subscriptions.skipped += 1;
            return Ok(());
        }
        let enabled = match self.mode {
            RestoreMode::Overwrite => s.enabled,
            _ => local.enabled,
        };
        let config = pick(self.mode, &local.config, &incoming);
        if enabled == local.enabled && config == local.config {
            self.report.notification_subscriptions.unchanged += 1;
            return Ok(());
        }
        notification_subscription::ActiveModel {
            id: Unchanged(local.id),
            enabled: Set(enabled),
            config: Set(config),
            ..Default::default()
        }
        .update(self.txn)
        .await?;
        self.report.notification_subscriptions.updated += 1;
        Ok(())
    }
}

async fn find_ref(
    txn: &DatabaseTransaction,
    provider: RefProvider,
    kind: crate::entity::enums::MediaKind,
    external_id: &str,
) -> Result<Option<media_ref::Model>, DbErr> {
    media_ref::Entity::find()
        .filter(media_ref::Column::Provider.eq(provider))
        .filter(media_ref::Column::Kind.eq(kind))
        .filter(media_ref::Column::ExternalId.eq(external_id))
        .one(txn)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn remap_rewrites_id_members_it_knows_and_nothing_else() {
        let old = Uuid::now_v7();
        let new = Uuid::now_v7();
        let other = Uuid::now_v7();
        let ids = HashMap::from([(old, new)]);
        let mut v = json!({
            "id": old,
            "run_id": old,
            "nested": {"entry_id": old, "notes": old.to_string()},
            "list": [{"media_id": old}],
            "user_id": other,
            "status": "watching",
        });
        remap(&mut v, &ids);
        assert_eq!(v["id"], json!(new));
        assert_eq!(v["run_id"], json!(new));
        assert_eq!(v["nested"]["entry_id"], json!(new));
        assert_eq!(v["list"][0]["media_id"], json!(new));
        assert_eq!(v["user_id"], json!(other), "an unmapped id is kept");
        assert_eq!(
            v["nested"]["notes"],
            json!(old.to_string()),
            "free text that happens to hold an id is not an id member"
        );
    }

    #[test]
    fn pick_follows_the_mode() {
        let (local, incoming) = (Some(1), Some(2));
        assert_eq!(pick(RestoreMode::Merge, &local, &incoming), Some(1));
        assert_eq!(pick(RestoreMode::Merge, &None, &incoming), Some(2));
        assert_eq!(pick(RestoreMode::Overwrite, &local, &incoming), Some(2));
        assert_eq!(pick(RestoreMode::Overwrite, &local, &None), None);
        assert_eq!(pick(RestoreMode::Skip, &None, &incoming), None);
    }
}
