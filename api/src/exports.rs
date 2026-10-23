//! Taking the caller's library out of Stacks: a typed JSON backup and a CSV.
//!
//! Both carry what core exports today: one row per live library entry, with
//! its status, score, current progress and first run's dates. The lossless
//! format (every run, play, note, tag and list) is issue #7's; the backup's
//! `version` changes when it arrives.

use kynos::extract::body::binary::Binary;
use kynos::http::media::MediaType;
use kynos::prelude::*;
use kynos::response::disposition::ContentDisposition;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::import_export::export::{self, BackupRow};

use crate::app::App;
use crate::catalog::Status;
use crate::ids::TitleRef;
use crate::problem::Internal;
use crate::security::{Caller, Reader};

/// Exports of the caller's library.
#[derive(Tag)]
#[tag(
    name = "exports",
    description = "The caller's library as a JSON backup or a CSV file."
)]
pub struct Exports;

/// The backup format this server writes.
pub const BACKUP_VERSION: u32 = 1;

/// One library entry in a backup.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct BackupEntry {
    pub title: TitleRef,
    pub status: Status,
    /// 0–100, or `null` when unscored.
    pub score: Option<i32>,
    /// Episodes watched in the current run, for a show.
    pub progress: Option<i64>,
    /// When the first watch started, if recorded.
    pub started_on: Option<chrono::NaiveDate>,
    /// When the first watch was completed, if recorded.
    pub completed_on: Option<chrono::NaiveDate>,
}

/// A backup of the caller's library.
///
/// Version 1 carries each live entry's status, score, current progress and
/// first run's dates. It does not carry rewatches, individual plays, notes,
/// tags, lists or the blocklist.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Backup {
    /// The backup format; 1 for this one.
    pub version: u32,
    pub exported_at: chrono::DateTime<chrono::Utc>,
    /// Every live entry, most recently updated first.
    pub entries: Vec<BackupEntry>,
}

/// `text/csv; charset=utf-8`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Csv;

impl MediaType for Csv {
    const MEDIA_TYPE: &'static str = "text/csv; charset=utf-8";
}

/// What an export can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ExportError {
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

fn entry(row: BackupRow) -> Option<BackupEntry> {
    let Some(media_id) = row.media_id else {
        tracing::warn!("a library entry whose title has no primary reference");
        return None;
    };
    Some(BackupEntry {
        title: TitleRef(media_id),
        status: row.status.into(),
        score: row.score.map(i32::from),
        progress: row.progress,
        started_on: row.started_on,
        completed_on: row.completed_on,
    })
}

fn attachment(stem: &str, extension: &str) -> ContentDisposition {
    let today = chrono::Utc::now().format("%Y-%m-%d");
    ContentDisposition::attachment().filename(format!("{stem}-{today}.{extension}"))
}

/// Download a backup of the caller's library as JSON.
#[kynos::get("/exports/backup", operation_id = "exportBackup")]
async fn export_backup(
    Auth(caller): Reader,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<WithHeaders<Json<Backup>, ContentDisposition>, ExportError> {
    let caller = Caller::from(caller);
    let rows = export::backup_rows(&db, caller.id())
        .await
        .map_err(Internal::log)?;
    let backup = Backup {
        version: BACKUP_VERSION,
        exported_at: chrono::Utc::now(),
        entries: rows.into_iter().filter_map(entry).collect(),
    };
    Ok(WithHeaders::new(
        Json(backup),
        attachment("stacks-backup", "json"),
    ))
}

/// Download the caller's library as CSV, one row per entry.
#[kynos::get("/exports/csv", operation_id = "exportCsv")]
async fn export_csv(
    Auth(caller): Reader,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<WithHeaders<Binary<Csv>, ContentDisposition>, ExportError> {
    let caller = Caller::from(caller);
    let rows = export::library_rows(&db, caller.id())
        .await
        .map_err(Internal::log)?;
    let csv = export::export_csv(&rows).map_err(Internal::log)?;
    Ok(WithHeaders::new(
        Binary::new(csv),
        attachment("stacks-library", "csv"),
    ))
}

pub(crate) fn group() -> Group<App> {
    Group::new("/")
        .tag::<Exports>()
        .mount(kynos::routes![export_backup, export_csv])
}
