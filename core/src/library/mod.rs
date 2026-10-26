//! The user's library: entries, watch runs, plays and the event log.
//!
//! The rules that keep it lossless:
//!
//! - every mutation is one transaction that also writes its `entry_events`
//!   row, so the log and the data can never disagree;
//! - updates are merge patches ([`EntryPatch`]): absent fields are untouched,
//!   and no code path writes a full row over an existing one;
//! - entries are soft-deleted and restorable;
//! - plays and events are append-only — [`plays`] and [`events`] only insert
//!   (and void), and undo voids a play;
//! - progress is derived from runs and plays (the `entry_progress` view).

mod completion;
pub mod episodes;
pub mod error;
pub mod events;
pub mod listing;
pub mod patch;
pub mod plays;
pub mod repo;
pub mod schedule;
pub mod service;

pub use error::LibraryError;
pub use events::EntryState;
pub use listing::{EntryPosition, LibraryFilter, LibrarySort};
pub use patch::{EntryPatch, RunDate, RunPatch};
#[cfg(feature = "test-support")]
pub use service::test_support::LogPlay;
pub use service::{EntryView, LibraryService, PlayBatch, WatchedEpisode};
