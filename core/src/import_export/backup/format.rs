//! The Stacks backup document, format version 2.
//!
//! These types are the format's definition: `docs/backup-format.schema.json`
//! is generated from them (`just gen-backup-schema`) and a test fails when
//! the committed schema drifts. `docs/backup-format.md` describes the
//! document and the restore procedure.
//!
//! A backup is lossless for everything a user owns: every library entry
//! (soft-deleted ones included), every watch run, every play (voided ones
//! included), the whole event log, lists with their order, tags and their
//! assignments, the blocklist, notification channels and their
//! subscriptions, and the user's settings. Every row keeps its id and its
//! timestamps. Catalog snapshots are not included: each title carries its
//! external references, which are enough to find or re-fetch it.

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use sea_orm::prelude::DateTimeWithTimeZone;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{
    ChannelType, DatePrecision, EntryEventKind, EntryStatus, MediaKind, NotificationEventType,
    PlaySource, RefProvider,
};
use crate::media::filter::FilterQuery;

/// The `format` member every backup carries.
pub const FORMAT: &str = "stacks-backup";

/// The format version this release writes, and the newest it reads.
pub const FORMAT_VERSION: u32 = 2;

/// A whole-account backup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(title = "Stacks backup", extend("$id" = "https://github.com/justin13888/Stacks/blob/master/docs/backup-format.schema.json"))]
pub struct Backup {
    /// Always `stacks-backup`.
    #[schemars(extend("const" = FORMAT))]
    pub format: String,
    /// The format version; this document describes version 2. A reader
    /// refuses a newer version and upgrades an older one.
    #[schemars(extend("const" = FORMAT_VERSION))]
    pub format_version: u32,
    pub exported_at: DateTime<Utc>,
    /// The Stacks release that wrote the backup, e.g. `stacks 0.1.0`.
    pub generator: String,
    /// Whether notification channel configurations are included. They hold
    /// credentials (webhook URLs, signing secrets, header values), so they
    /// are left out unless the export asked for them.
    pub secrets_included: bool,
    /// The account the backup was taken from. Absent only in a document
    /// upgraded from version 0, which did not record it.
    pub user: Option<Profile>,
    /// The user's preferences, as stored (`users.settings`).
    pub settings: serde_json::Map<String, serde_json::Value>,
    /// Every title any entry, list item or blocklist row refers to.
    pub titles: Vec<Title>,
    /// Library entries, soft-deleted ones included.
    pub entries: Vec<Entry>,
    /// Lists, soft-deleted ones included.
    pub lists: Vec<List>,
    pub tags: Vec<Tag>,
    pub blocklist: Vec<Blocked>,
    pub notification_channels: Vec<Channel>,
}

/// Who the backup belongs to. Informational: a restore applies the backup to
/// whoever restores it and never changes their identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Profile {
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub oidc_issuer: String,
    pub oidc_subject: String,
    pub created_at: DateTimeWithTimeZone,
}

/// A title, by the external ids that name it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Title {
    /// The title's catalog id where the backup was taken; entries, list
    /// items and blocklist rows refer to it.
    pub id: Uuid,
    /// The title's name when the backup was taken, so the document reads
    /// without a catalog. A restore re-fetches everything else.
    pub name: String,
    pub kind: MediaKind,
    /// Every external id of the title. Exactly one is `primary`: the public
    /// id (`tmdb:movie:550`, `anilist:show:21`).
    pub refs: Vec<TitleRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TitleRef {
    pub provider: RefProvider,
    pub kind: MediaKind,
    pub external_id: String,
    pub primary: bool,
}

/// A library entry with its runs, its event log and its tags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Entry {
    pub id: Uuid,
    /// The [`Title::id`] it tracks.
    pub title_id: Uuid,
    pub status: EntryStatus,
    /// 0–100.
    #[schemars(range(min = 0, max = 100))]
    pub score: Option<i16>,
    pub favourite: bool,
    pub notes: Option<String>,
    /// Set when the entry is in the trash.
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// Watch-throughs, first watch first.
    pub runs: Vec<Run>,
    /// The entry's append-only event log, oldest first. `before` and `after`
    /// are the rows as they were stored, so they keep whatever members the
    /// release that wrote them had.
    pub events: Vec<Event>,
    /// The tags assigned to the entry.
    pub tags: Vec<EntryTag>,
}

/// One watch-through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Run {
    pub id: Uuid,
    /// 1 for the first watch; later runs are rewatches.
    #[schemars(range(min = 1))]
    pub number: i32,
    pub started_on: Option<NaiveDate>,
    pub started_precision: Option<DatePrecision>,
    pub completed_on: Option<NaiveDate>,
    pub completed_precision: Option<DatePrecision>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// Every play of the run, oldest first, voided ones included.
    pub plays: Vec<Play>,
}

/// One recorded viewing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Play {
    pub id: Uuid,
    /// The episode watched; absent for a movie.
    pub episode: Option<EpisodeRef>,
    pub watched_at: Option<DateTimeWithTimeZone>,
    pub source: PlaySource,
    /// The import the play came from, where it was recorded.
    pub import_id: Option<Uuid>,
    /// Set when the play was undone; a voided play is kept.
    pub voided_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

/// An episode, by its numbers within the title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EpisodeRef {
    /// The episode's catalog id where the backup was taken (event payloads
    /// refer to it).
    pub id: Uuid,
    pub season: i32,
    pub number: i32,
}

/// One row of the entry's event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub id: Uuid,
    pub kind: EntryEventKind,
    pub before: Option<serde_json::Value>,
    pub after: Option<serde_json::Value>,
    pub created_at: DateTimeWithTimeZone,
}

/// A tag assigned to an entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EntryTag {
    pub id: Uuid,
    /// The [`Tag::id`] assigned.
    pub tag_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct List {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// A smart list is defined by `filter` rather than by its items.
    pub smart: bool,
    /// A smart list's filter. Its `tag` members are [`Tag::id`]s; one the
    /// backup lacks names a tag deleted since, and matches nothing.
    pub filter: Option<FilterQuery>,
    pub deleted_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    /// The items in list order.
    pub items: Vec<ListItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListItem {
    pub id: Uuid,
    pub title_id: Uuid,
    /// The item's fractional-index order key, compared bytewise.
    #[schemars(regex(pattern = r"^[0-9A-Za-z]*[1-9A-Za-z]$"))]
    pub position: String,
    pub notes: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tag {
    pub id: Uuid,
    pub name: String,
    pub color: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

/// A title the user never wants suggested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Blocked {
    pub id: Uuid,
    pub title_id: Uuid,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

/// A notification channel with its subscriptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Channel {
    pub id: Uuid,
    pub channel_type: ChannelType,
    pub name: String,
    pub enabled: bool,
    /// The channel's configuration; `null` when the backup was taken
    /// without secrets. A restore skips a channel without one.
    pub config: Option<serde_json::Map<String, serde_json::Value>>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub subscriptions: Vec<Subscription>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Subscription {
    pub id: Uuid,
    pub event_type: NotificationEventType,
    pub enabled: bool,
    pub config: Option<serde_json::Map<String, serde_json::Value>>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

/// The JSON Schema of the current format, as committed in
/// `docs/backup-format.schema.json`.
pub fn json_schema() -> serde_json::Value {
    schemars::schema_for!(Backup).to_value()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_committed_schema_is_current() {
        let committed = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../docs/backup-format.schema.json"
        ))
        .expect("docs/backup-format.schema.json is committed");
        let committed: serde_json::Value =
            serde_json::from_str(&committed).expect("the committed schema is JSON");
        assert!(
            committed == super::json_schema(),
            "docs/backup-format.schema.json is stale; regenerate it with `just gen-backup-schema`"
        );
    }
}
