//! The append-only `entry_events` log.
//!
//! This module only inserts and reads. An event is never changed or removed,
//! and the database refuses both (see the baseline migration).
//!
//! An event stores the rows it changed, before and after, as JSON. Reading
//! one back goes through [`describe`], which decodes only the members it
//! needs (so a row stored by an older release, with fewer columns, still
//! reads) into a typed [`Change`].

use std::collections::HashMap;

use chrono::NaiveDate;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder, QuerySelect,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{DatePrecision, EntryEventKind, EntryStatus, PlaySource};
use crate::entity::{entry_event, watch_run};
use crate::media::catalog;
use crate::pagination::{Slice, overfetch};

use super::patch::RunDate;

/// Append one event. Call it with the transaction that made the change.
pub async fn record<C, B, A>(
    db: &C,
    entry_id: Uuid,
    user_id: Uuid,
    kind: EntryEventKind,
    before: Option<&B>,
    after: Option<&A>,
) -> Result<entry_event::Model, DbErr>
where
    C: ConnectionTrait,
    B: Serialize,
    A: Serialize,
{
    entry_event::ActiveModel {
        id: Set(Uuid::now_v7()),
        entry_id: Set(entry_id),
        user_id: Set(user_id),
        kind: Set(kind),
        before: Set(before.map(to_json).transpose()?),
        after: Set(after.map(to_json).transpose()?),
        created_at: NotSet,
    }
    .insert(db)
    .await
}

/// Append many events at once, all with no `before`. Call it with the
/// transaction that made the change.
pub async fn record_many<C, A>(
    db: &C,
    user_id: Uuid,
    kind: EntryEventKind,
    afters: &[(Uuid, A)],
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
    A: Serialize,
{
    if afters.is_empty() {
        return Ok(());
    }
    let rows = afters
        .iter()
        .map(|(entry_id, after)| {
            Ok(entry_event::ActiveModel {
                id: Set(Uuid::now_v7()),
                entry_id: Set(*entry_id),
                user_id: Set(user_id),
                kind: Set(kind),
                before: Set(None),
                after: Set(Some(to_json(after)?)),
                created_at: NotSet,
            })
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    entry_event::Entity::insert_many(rows).exec(db).await?;
    Ok(())
}

fn to_json<T: Serialize>(v: &T) -> Result<serde_json::Value, DbErr> {
    serde_json::to_value(v).map_err(|e| DbErr::Custom(format!("event payload: {e}")))
}

/// One window of a user's events, newest first, optionally for one entry,
/// continuing after the event `after`.
///
/// Keyset-paginated on the UUIDv7 id, which is time-ordered.
pub async fn list<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    entry_id: Option<Uuid>,
    after: Option<Uuid>,
    limit: u64,
) -> Result<Slice<entry_event::Model>, DbErr> {
    let mut query = entry_event::Entity::find()
        .filter(entry_event::Column::UserId.eq(user_id))
        .order_by(entry_event::Column::Id, Order::Desc);
    if let Some(entry_id) = entry_id {
        query = query.filter(entry_event::Column::EntryId.eq(entry_id));
    }
    if let Some(after) = after {
        query = query.filter(entry_event::Column::Id.lt(after));
    }
    let items = query.limit(overfetch(limit)).all(db).await?;
    Ok(Slice::from_overfetch(items, limit))
}

/// Every event for one entry, oldest first.
pub async fn for_entry<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
) -> Result<Vec<entry_event::Model>, DbErr> {
    entry_event::Entity::find()
        .filter(entry_event::Column::EntryId.eq(entry_id))
        .order_by(entry_event::Column::Id, Order::Asc)
        .all(db)
        .await
}

// ------------------------------------------------------------- reading back

/// An entry's user-set state at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryState {
    pub status: EntryStatus,
    pub score: Option<i16>,
    pub favourite: bool,
    pub notes: Option<String>,
}

/// A run's number and dates at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunState {
    pub number: i32,
    pub started: Option<RunDate>,
    pub completed: Option<RunDate>,
}

/// A play as it stood when the event was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayState {
    pub id: Uuid,
    /// The number of the run it belongs to.
    pub run: i32,
    /// `(season, episode)` for an episode; `None` for a movie.
    pub episode: Option<(i32, i32)>,
    pub watched_at: Option<DateTimeWithTimeZone>,
    pub source: PlaySource,
    pub voided_at: Option<DateTimeWithTimeZone>,
    /// When it was logged.
    pub logged_at: DateTimeWithTimeZone,
}

/// What one event changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    EntryCreated(EntryState),
    EntryUpdated {
        before: EntryState,
        after: EntryState,
    },
    EntryDeleted,
    EntryRestored,
    RunStarted(RunState),
    RunUpdated {
        before: RunState,
        after: RunState,
    },
    PlayLogged(PlayState),
    PlayVoided(PlayState),
}

/// An event with its change decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Described {
    pub event: entry_event::Model,
    pub change: Change,
}

#[derive(Deserialize)]
struct StoredEntry {
    status: EntryStatus,
    score: Option<i16>,
    favourite: bool,
    notes: Option<String>,
}

impl From<StoredEntry> for EntryState {
    fn from(e: StoredEntry) -> Self {
        Self {
            status: e.status,
            score: e.score,
            favourite: e.favourite,
            notes: e.notes,
        }
    }
}

#[derive(Deserialize)]
struct StoredRun {
    run_number: i32,
    started_on: Option<NaiveDate>,
    started_precision: Option<DatePrecision>,
    completed_on: Option<NaiveDate>,
    completed_precision: Option<DatePrecision>,
}

fn run_date(date: Option<NaiveDate>, precision: Option<DatePrecision>) -> Option<RunDate> {
    Some(RunDate {
        date: date?,
        precision: precision?,
    })
}

impl From<StoredRun> for RunState {
    fn from(r: StoredRun) -> Self {
        Self {
            number: r.run_number,
            started: run_date(r.started_on, r.started_precision),
            completed: run_date(r.completed_on, r.completed_precision),
        }
    }
}

#[derive(Deserialize)]
struct StoredPlay {
    id: Uuid,
    run_id: Uuid,
    episode_id: Option<Uuid>,
    watched_at: Option<DateTimeWithTimeZone>,
    source: PlaySource,
    voided_at: Option<DateTimeWithTimeZone>,
    created_at: DateTimeWithTimeZone,
}

fn decode<T: DeserializeOwned>(
    event: &entry_event::Model,
    side: Option<&serde_json::Value>,
) -> Result<T, DbErr> {
    let value = side.ok_or_else(|| {
        DbErr::Custom(format!(
            "event {} ({}) has no payload",
            event.id, event.kind
        ))
    })?;
    T::deserialize(value)
        .map_err(|e| DbErr::Custom(format!("event {} ({}): {e}", event.id, event.kind)))
}

/// Decode a batch of events, resolving the run numbers and episode numbers
/// their plays refer to in a fixed number of queries.
pub async fn describe<C: ConnectionTrait>(
    db: &C,
    events: Vec<entry_event::Model>,
) -> Result<Vec<Described>, DbErr> {
    let mut plays: HashMap<Uuid, StoredPlay> = HashMap::new();
    for event in &events {
        if matches!(
            event.kind,
            EntryEventKind::PlayLogged | EntryEventKind::PlayVoided
        ) {
            plays.insert(event.id, decode(event, event.after.as_ref())?);
        }
    }
    let run_ids: Vec<Uuid> = plays.values().map(|p| p.run_id).collect();
    let runs: HashMap<Uuid, i32> = if run_ids.is_empty() {
        HashMap::new()
    } else {
        watch_run::Entity::find()
            .filter(watch_run::Column::Id.is_in(run_ids))
            .all(db)
            .await?
            .into_iter()
            .map(|r| (r.id, r.run_number))
            .collect()
    };
    let episode_ids: Vec<Uuid> = plays.values().filter_map(|p| p.episode_id).collect();
    let episodes = catalog::episode_numbers(db, &episode_ids).await?;

    let mut out = Vec::with_capacity(events.len());
    for event in events {
        let entry = |side: Option<&serde_json::Value>| -> Result<EntryState, DbErr> {
            decode::<StoredEntry>(&event, side).map(EntryState::from)
        };
        let run = |side: Option<&serde_json::Value>| -> Result<RunState, DbErr> {
            decode::<StoredRun>(&event, side).map(RunState::from)
        };
        let mut play = || -> Result<PlayState, DbErr> {
            let p = plays
                .remove(&event.id)
                .ok_or_else(|| DbErr::Custom(format!("event {} has no play", event.id)))?;
            // Runs cannot be deleted, so a play's run is always there.
            let run = runs.get(&p.run_id).copied().ok_or_else(|| {
                DbErr::RecordNotFound(format!("run {} of play {}", p.run_id, p.id))
            })?;
            Ok(PlayState {
                id: p.id,
                run,
                episode: p.episode_id.and_then(|e| episodes.get(&e).copied()),
                watched_at: p.watched_at,
                source: p.source,
                voided_at: p.voided_at,
                logged_at: p.created_at,
            })
        };
        let change = match event.kind {
            EntryEventKind::EntryCreated => Change::EntryCreated(entry(event.after.as_ref())?),
            EntryEventKind::EntryUpdated => Change::EntryUpdated {
                before: entry(event.before.as_ref())?,
                after: entry(event.after.as_ref())?,
            },
            EntryEventKind::EntryDeleted => Change::EntryDeleted,
            EntryEventKind::EntryRestored => Change::EntryRestored,
            EntryEventKind::RunStarted => Change::RunStarted(run(event.after.as_ref())?),
            EntryEventKind::RunUpdated => Change::RunUpdated {
                before: run(event.before.as_ref())?,
                after: run(event.after.as_ref())?,
            },
            EntryEventKind::PlayLogged => Change::PlayLogged(play()?),
            EntryEventKind::PlayVoided => Change::PlayVoided(play()?),
        };
        out.push(Described { event, change });
    }
    Ok(out)
}
