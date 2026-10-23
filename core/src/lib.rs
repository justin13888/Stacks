pub mod auth;
pub mod blocklist;
pub mod config;
pub mod db;
pub mod entity;
pub mod import_export;
pub mod jobs;
pub mod library;
pub mod lists;
pub mod media;
pub mod notifications;
pub mod pagination;
pub mod patch;
pub mod stats;
pub mod tags;
pub mod user;

/// Re-export of the `cameo` media-metadata SDK, so the API crate can name
/// cameo types without taking a direct dependency on the crate.
pub use cameo;
pub use patch::Patch;
/// Re-exported so callers can name ids without their own `uuid` dependency.
pub use uuid::Uuid;
