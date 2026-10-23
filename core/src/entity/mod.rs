//! SeaORM entities — one module per table (plus the `entry_progress` view).
//!
//! The schema itself lives in the baseline migration
//! (`core/migration/src/m20261101_000001_baseline.rs`); these types mirror it.

pub mod api_token;
pub mod blocklist;
pub mod entry_event;
pub mod entry_progress;
pub mod entry_tag;
pub mod enums;
pub mod episode;
pub mod import;
pub mod import_item;
pub mod job;
pub mod library_entry;
pub mod list;
pub mod list_item;
pub mod media;
pub mod media_ref;
pub mod notification_channel;
pub mod notification_subscription;
pub mod oidc_login_state;
pub mod play;
pub mod season;
pub mod session;
pub mod tag;
pub mod user;
pub mod watch_run;
