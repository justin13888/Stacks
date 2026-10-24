//! What the library's operations read and write.

use chrono::{DateTime, NaiveDate, Utc};
use kynos::prelude::*;
use serde::{Deserialize, Serialize};
use stacks_core::entity::enums::{DatePrecision, PlaySource as CorePlaySource};
use stacks_core::entity::{play, watch_run};
use stacks_core::library::events::{Change as CoreChange, PlayState, RunState};
use stacks_core::library::{EntryState, EntryView, LibrarySort, RunDate as CoreRunDate};
use uuid::Uuid;

use crate::catalog::{Category, Status, TitleSummary};
use crate::ids::TitleRef;
use crate::patch::Nullable;

fn utc(t: sea_orm::prelude::DateTimeWithTimeZone) -> DateTime<Utc> {
    t.to_utc()
}

fn count(n: i64) -> u64 {
    u64::try_from(n).unwrap_or_default()
}

fn number(n: i32) -> u32 {
    u32::try_from(n).unwrap_or_default()
}

// ------------------------------------------------------------------ dates

/// How precisely a date is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precision {
    Day,
    /// Only the month: the day of `date` means nothing.
    Month,
    /// Only the year: the month and day of `date` mean nothing.
    Year,
}

/// A date and how precisely it is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RunDate {
    pub date: NaiveDate,
    pub precision: Precision,
}

impl From<CoreRunDate> for RunDate {
    fn from(d: CoreRunDate) -> Self {
        Self {
            date: d.date,
            precision: match d.precision {
                DatePrecision::Day => Precision::Day,
                DatePrecision::Month => Precision::Month,
                DatePrecision::Year => Precision::Year,
            },
        }
    }
}

impl From<RunDate> for CoreRunDate {
    fn from(d: RunDate) -> Self {
        Self {
            date: d.date,
            precision: match d.precision {
                Precision::Day => DatePrecision::Day,
                Precision::Month => DatePrecision::Month,
                Precision::Year => DatePrecision::Year,
            },
        }
    }
}

fn run_date(date: Option<NaiveDate>, precision: Option<DatePrecision>) -> Option<RunDate> {
    Some(
        CoreRunDate {
            date: date?,
            precision: precision?,
        }
        .into(),
    )
}

// ----------------------------------------------------------------- entries

/// One watch-through of a title: run 1 is the first watch, later runs are
/// rewatches.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct WatchRun {
    pub number: u32,
    /// `null` when unknown.
    pub started: Option<RunDate>,
    /// `null` while the run is under way, or when unknown.
    pub completed: Option<RunDate>,
}

impl From<&watch_run::Model> for WatchRun {
    fn from(r: &watch_run::Model) -> Self {
        Self {
            number: number(r.run_number),
            started: run_date(r.started_on, r.started_precision),
            completed: run_date(r.completed_on, r.completed_precision),
        }
    }
}

impl From<RunState> for WatchRun {
    fn from(r: RunState) -> Self {
        Self {
            number: number(r.number),
            started: r.started.map(Into::into),
            completed: r.completed.map(Into::into),
        }
    }
}

/// How far the caller has got, derived from runs and plays (voided plays
/// never count).
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct Progress {
    pub runs: u32,
    pub rewatches: u32,
    /// Live plays across every run.
    pub plays: u64,
    /// Live plays in the current run.
    pub current_run_plays: u64,
    /// Distinct episodes watched in the current run.
    pub current_run_episodes: u64,
    pub first_watched_at: Option<DateTime<Utc>>,
    pub last_watched_at: Option<DateTime<Utc>>,
}

/// A title in the caller's library.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub title: TitleSummary,
    pub status: Status,
    /// 0–100, or `null` when unscored.
    pub score: Option<u8>,
    pub favourite: bool,
    pub notes: Option<String>,
    pub progress: Progress,
    /// When the title was first added.
    pub added_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// When the entry went to the trash; `null` while it is in the library.
    pub deleted_at: Option<DateTime<Utc>>,
}

impl LibraryEntry {
    pub fn new(title: TitleSummary, view: &EntryView) -> Self {
        let e = &view.entry;
        let progress = view.progress.as_ref();
        Self {
            title,
            status: e.status.into(),
            score: e.score.and_then(|s| u8::try_from(s).ok()),
            favourite: e.favourite,
            notes: e.notes.clone(),
            progress: Progress {
                runs: progress.map_or(0, |p| number(p.run_count)),
                rewatches: progress.map_or(0, |p| number(p.rewatch_count)),
                plays: progress.map_or(0, |p| count(p.play_count)),
                current_run_plays: progress.map_or(0, |p| count(p.current_run_plays)),
                current_run_episodes: progress.map_or(0, |p| count(p.current_run_episodes)),
                first_watched_at: progress.and_then(|p| p.first_watched_at).map(utc),
                last_watched_at: progress.and_then(|p| p.last_watched_at).map(utc),
            },
            added_at: utc(e.created_at),
            updated_at: utc(e.updated_at),
            deleted_at: e.deleted_at.map(utc),
        }
    }
}

/// An episode watched in the current run.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct WatchedEpisode {
    pub season: u32,
    pub episode: u32,
    /// When it was last watched in the run, if known.
    pub watched_at: Option<DateTime<Utc>>,
}

/// A library entry with its runs and what the current run has watched.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct LibraryEntryDetail {
    #[serde(flatten)]
    pub entry: LibraryEntry,
    /// First watch first.
    pub runs: Vec<WatchRun>,
    /// The episodes watched in the current run, by season then episode.
    pub watched: Vec<WatchedEpisode>,
}

/// An entry to store: every member is set, and a member left out is cleared
/// (or `false`).
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct EntryBody {
    pub status: Status,
    /// 0–100; `null` or absent for unscored.
    #[schema(maximum = 100)]
    pub score: Option<u8>,
    #[serde(default)]
    pub favourite: bool,
    #[schema(max_length = 10000)]
    pub notes: Option<String>,
}

impl EntryBody {
    pub fn into_state(self) -> EntryState {
        EntryState {
            status: self.status.into(),
            score: self.score.map(i16::from),
            favourite: self.favourite,
            notes: self.notes,
        }
    }
}

/// A change to an entry. A member left out is unchanged; `null` clears
/// `score` or `notes`.
#[derive(Debug, Default, Schema, Deserialize)]
pub struct EntryChanges {
    pub status: Option<Status>,
    /// 0–100, or `null` to clear.
    #[serde(default)]
    #[schema(maximum = 100)]
    pub score: Nullable<u8>,
    pub favourite: Option<bool>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub notes: Nullable<String>,
}

impl EntryChanges {
    pub fn into_patch(self) -> stacks_core::library::EntryPatch {
        stacks_core::library::EntryPatch {
            status: self.status.map(Into::into),
            score: self.score.map(i16::from).into_patch(),
            favourite: self.favourite,
            notes: self.notes.into_patch(),
        }
    }
}

/// A rewatch to start.
#[derive(Debug, Default, Schema, Serialize, Deserialize)]
pub struct NewRun {
    /// When it started; today when left out.
    pub started: Option<RunDate>,
}

/// A change to a run's dates. A member left out is unchanged; `null` clears
/// it.
#[derive(Debug, Default, Schema, Deserialize)]
pub struct RunChangesFields {
    #[serde(default)]
    pub started: Nullable<RunDate>,
    #[serde(default)]
    pub completed: Nullable<RunDate>,
}

/// [`RunChangesFields`], read only from a JSON object.
///
/// Every member of the fields has a default, and serde's derive then also
/// reads the struct from an empty array; `[]` is not a change to a run, so
/// this reads an object first. The description is the fields'.
#[derive(Debug, Default)]
pub struct RunChanges(pub RunChangesFields);

impl<'de> Deserialize<'de> for RunChanges {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let object = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        RunChangesFields::deserialize(serde_json::Value::Object(object))
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

impl Schema for RunChanges {
    fn schema(registry: &mut kynos::schema::registry::Registry) -> kynos::openapi::Schema {
        <RunChangesFields as Schema>::schema(registry)
    }

    fn name() -> Option<kynos::openapi::ComponentName> {
        kynos::openapi::ComponentName::sanitized("RunChanges").ok()
    }

    fn check_constraints(
        &self,
        at: kynos::schema::constraints::Pointer<'_>,
        violations: &mut kynos::schema::constraints::Violations,
    ) {
        self.0.check_constraints(at, violations);
    }
}

impl RunChanges {
    pub fn into_patch(self) -> stacks_core::library::RunPatch {
        stacks_core::library::RunPatch {
            started: self.0.started.map(Into::into).into_patch(),
            completed: self.0.completed.map(Into::into).into_patch(),
        }
    }
}

// ------------------------------------------------------------------- plays

/// How a play was recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaySource {
    /// One play, logged by hand.
    Manual,
    /// Part of marking a run of episodes watched at once.
    Bulk,
    /// From an import.
    Import,
}

impl From<CorePlaySource> for PlaySource {
    fn from(s: CorePlaySource) -> Self {
        match s {
            CorePlaySource::Manual => Self::Manual,
            CorePlaySource::Bulk => Self::Bulk,
            CorePlaySource::Import => Self::Import,
        }
    }
}

/// One recorded viewing of a movie or an episode.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct Play {
    /// Pass to `voidPlay` to undo it.
    pub id: Uuid,
    /// The number of the run it belongs to.
    pub run: u32,
    /// `null` for a movie.
    pub season: Option<u32>,
    /// `null` for a movie.
    pub episode: Option<u32>,
    /// When it was watched; `null` when unknown.
    pub watched_at: Option<DateTime<Utc>>,
    pub source: PlaySource,
    /// When it was logged.
    pub logged_at: DateTime<Utc>,
    /// When it was undone; `null` while it counts.
    pub voided_at: Option<DateTime<Utc>>,
}

impl Play {
    pub fn new(p: &play::Model, run: i32, episode: Option<(i32, i32)>) -> Self {
        Self {
            id: p.id,
            run: number(run),
            season: episode.map(|(s, _)| number(s)),
            episode: episode.map(|(_, e)| number(e)),
            watched_at: p.watched_at.map(utc),
            source: p.source.into(),
            logged_at: utc(p.created_at),
            voided_at: p.voided_at.map(utc),
        }
    }
}

impl From<PlayState> for Play {
    fn from(p: PlayState) -> Self {
        Self {
            id: p.id,
            run: number(p.run),
            season: p.episode.map(|(s, _)| number(s)),
            episode: p.episode.map(|(_, e)| number(e)),
            watched_at: p.watched_at.map(utc),
            source: p.source.into(),
            logged_at: utc(p.logged_at),
            voided_at: p.voided_at.map(utc),
        }
    }
}

/// One episode of a show, by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct EpisodeNumber {
    /// Specials are season 0.
    #[schema(maximum = 10000)]
    pub season: u32,
    #[schema(maximum = 10000)]
    pub episode: u32,
}

/// The plays to log, by kind.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewPlays {
    /// One play of a movie.
    Movie {
        /// When it was watched; now when left out.
        watched_at: Option<DateTime<Utc>>,
    },
    /// One play of each listed episode of a show, in order.
    Episodes {
        #[schema(min_items = 1, max_items = 500)]
        episodes: Vec<EpisodeNumber>,
        /// When they were watched; now when left out.
        watched_at: Option<DateTime<Utc>>,
    },
    /// One play of every episode of the current run not yet watched, from
    /// season 1 episode 1 up to and including this one. Specials (season 0)
    /// are not included.
    Through {
        #[schema(minimum = 1, maximum = 10000)]
        season: u32,
        #[schema(maximum = 10000)]
        episode: u32,
        /// When they were watched; unknown (`null`) when left out.
        watched_at: Option<DateTime<Utc>>,
    },
}

/// The plays a request logged.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct LoggedPlays {
    /// In the order logged; empty when every episode was already watched.
    pub plays: Vec<Play>,
}

// ------------------------------------------------------------------ events

/// An entry's own settings at one moment.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct EntrySettings {
    pub status: Status,
    pub score: Option<u8>,
    pub favourite: bool,
    pub notes: Option<String>,
}

impl From<EntryState> for EntrySettings {
    fn from(e: EntryState) -> Self {
        Self {
            status: e.status.into(),
            score: e.score.and_then(|s| u8::try_from(s).ok()),
            favourite: e.favourite,
            notes: e.notes,
        }
    }
}

/// What an event changed, by kind.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    /// The title was added to the library.
    EntryCreated { entry: EntrySettings },
    /// The entry's settings changed.
    EntryUpdated {
        before: EntrySettings,
        after: EntrySettings,
    },
    /// The entry went to the trash.
    EntryDeleted,
    /// The entry came back from the trash.
    EntryRestored,
    /// A run began.
    RunStarted { run: WatchRun },
    /// A run's dates changed.
    RunUpdated { before: WatchRun, after: WatchRun },
    /// A play was logged; `play` is as it was logged.
    PlayLogged { play: Play },
    /// A play was voided.
    PlayVoided { play: Play },
}

impl From<CoreChange> for Change {
    fn from(c: CoreChange) -> Self {
        match c {
            CoreChange::EntryCreated(e) => Self::EntryCreated { entry: e.into() },
            CoreChange::EntryUpdated { before, after } => Self::EntryUpdated {
                before: before.into(),
                after: after.into(),
            },
            CoreChange::EntryDeleted => Self::EntryDeleted,
            CoreChange::EntryRestored => Self::EntryRestored,
            CoreChange::RunStarted(r) => Self::RunStarted { run: r.into() },
            CoreChange::RunUpdated { before, after } => Self::RunUpdated {
                before: before.into(),
                after: after.into(),
            },
            CoreChange::PlayLogged(p) => Self::PlayLogged { play: p.into() },
            CoreChange::PlayVoided(p) => Self::PlayVoided { play: p.into() },
        }
    }
}

/// One change to a library entry, from its append-only log.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct EntryEvent {
    pub id: Uuid,
    /// When it happened.
    pub at: DateTime<Utc>,
    pub change: Change,
}

impl From<stacks_core::library::events::Described> for EntryEvent {
    fn from(d: stacks_core::library::events::Described) -> Self {
        Self {
            id: d.event.id,
            at: utc(d.event.created_at),
            change: d.change.into(),
        }
    }
}

/// An event in the caller's history, with the title it concerns.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct HistoryEvent {
    pub title: TitleSummary,
    pub event: EntryEvent,
}

// ------------------------------------------------------- up next, calendar

/// The next episode of a show the caller is watching.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct UpNextItem {
    pub title: TitleSummary,
    pub season: u32,
    pub episode: u32,
    /// `null` until the episode's details are known.
    pub name: Option<String>,
    pub air_date: Option<NaiveDate>,
    /// Whether it has aired: `false` when its air date is later or unknown.
    pub aired: bool,
    /// When the caller last watched anything of the show.
    pub last_watched_at: Option<DateTime<Utc>>,
}

/// An episode airing for a show in the caller's library.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct CalendarEpisode {
    pub title: TitleSummary,
    pub season: u32,
    pub episode: u32,
    pub name: Option<String>,
    pub air_date: NaiveDate,
    pub runtime_minutes: Option<u32>,
    /// Whether the caller has watched it in the current run.
    pub watched: bool,
}

// -------------------------------------------------------------- parameters

/// What `/library/{ref}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct EntryPath {
    /// The title's reference, such as `tmdb:movie:550`.
    #[param(rename = "ref")]
    pub reference: TitleRef,
}

/// What `/library/{ref}/runs/{number}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct RunPath {
    /// The title's reference, such as `tmdb:show:1396`.
    #[param(rename = "ref")]
    pub reference: TitleRef,
    /// The run's number: 1 for the first watch.
    #[schema(minimum = 1, maximum = 10000)]
    pub number: u32,
}

/// What `/plays/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct PlayPath {
    pub id: Uuid,
}

/// The order of the library listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    /// Most recently changed first.
    Updated,
    /// Most recently added first.
    Added,
    /// By title, A to Z.
    Title,
    /// Highest score first, unscored last.
    Score,
    /// Furthest along the current run first: the distinct episodes it has
    /// watched (`progress.current_run_episodes`); a movie the current run
    /// has watched counts as one.
    Progress,
    /// The current run most recently started first (its `started` date);
    /// entries whose current run has no start date, or that have no run,
    /// last.
    Started,
}

impl From<Sort> for LibrarySort {
    fn from(s: Sort) -> Self {
        match s {
            Sort::Updated => Self::Updated,
            Sort::Added => Self::Added,
            Sort::Title => Self::Title,
            Sort::Score => Self::Score,
            Sort::Progress => Self::Progress,
            Sort::Started => Self::Started,
        }
    }
}

/// How the library is listed.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct LibraryParams {
    /// Only entries with this status.
    pub status: Option<Status>,
    /// Only titles of this category.
    pub category: Option<Category>,
    /// Only favourites (`true`), or only the rest (`false`).
    pub favourite: Option<bool>,
    /// Only entries carrying this tag.
    pub tag: Option<Uuid>,
    /// Only titles whose title, original title or romaji title contains this
    /// text, ignoring case.
    #[schema(min_length = 1, max_length = 200)]
    pub q: Option<String>,
    /// The order (default `updated`).
    pub sort: Option<Sort>,
    /// `true` lists the trash instead.
    pub deleted: Option<bool>,
    /// The `next_cursor` of the previous page. Omit for the first page. A
    /// cursor only continues the listing (filters and order) it came from.
    pub cursor: Option<String>,
    /// At most this many items (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u32>,
}

/// Which days the calendar covers.
#[derive(Debug, Schema, QueryParams)]
pub struct CalendarParams {
    /// The first day, inclusive.
    pub from: NaiveDate,
    /// The last day, inclusive: on or after `from`, and at most 62 days
    /// after it.
    pub to: NaiveDate,
    /// The `next_cursor` of the previous page. Omit for the first page.
    pub cursor: Option<String>,
    /// At most this many items (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u32>,
}
