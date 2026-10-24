//! Taking the caller's data out of Stacks: the lossless JSON backup and a
//! flat CSV of the library.
//!
//! The backup is a file in the Stacks backup format
//! (`docs/backup-format.md`, with its JSON Schema in
//! `docs/backup-format.schema.json`): every entry, run, play, event, list,
//! tag, blocklist row and notification channel the caller owns. It is
//! restored with `restoreBackup`. The CSV is for spreadsheets: one row per
//! live entry, lossy by design, and not importable.

use kynos::extract::body::binary::Binary;
use kynos::http::media::MediaType;
use kynos::prelude::*;
use kynos::response::disposition::ContentDisposition;
use sea_orm::DatabaseConnection;
use stacks_core::import_export::backup::{self, ExportOptions};
use stacks_core::import_export::export;

use crate::app::App;
use crate::problem::Internal;
use crate::security::{Caller, Reader};

/// Exports of the caller's data.
#[derive(Tag)]
#[tag(
    name = "exports",
    description = "The caller's data as a lossless JSON backup, or their library as a CSV file."
)]
pub struct Exports;

/// `application/json`: a Stacks backup document, described by
/// `docs/backup-format.schema.json` rather than by this API's schemas.
#[derive(Clone, Copy, Debug, Default)]
pub struct BackupFile;

impl MediaType for BackupFile {
    const MEDIA_TYPE: &'static str = "application/json";
}

/// `text/csv; charset=utf-8`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Csv;

impl MediaType for Csv {
    const MEDIA_TYPE: &'static str = "text/csv; charset=utf-8";
}

/// What the backup includes.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct BackupParams {
    /// Include notification channel configurations. They hold credentials
    /// (webhook URLs, signing secrets, header values), so they are left out
    /// unless this is `true`; a restore skips a channel without one. Only a
    /// signed-in session may ask: an API token is refused, so a leaked token
    /// never reads back a credential the channel API itself never returns.
    pub include_secrets: Option<bool>,
}

/// What a backup export can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum BackupError {
    /// `include_secrets=true` came with an API token. Credentials leave
    /// Stacks only through a signed-in session (ADR 0004).
    #[error("only a signed-in session can export notification channel secrets")]
    #[problem(status = 403, title = "Secrets need a session")]
    SecretsNeedSession,
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What a CSV export can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ExportError {
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

fn attachment(stem: &str, extension: &str) -> ContentDisposition {
    let today = chrono::Utc::now().format("%Y-%m-%d");
    ContentDisposition::attachment().filename(format!("{stem}-{today}.{extension}"))
}

/// Download a lossless backup of everything the caller owns.
///
/// The file is a Stacks backup document (format version 1); restore it
/// with `restoreBackup`. Any `read` token may take a backup, but only a
/// signed-in session may add `include_secrets=true`.
#[kynos::get("/exports/backup", operation_id = "exportBackup")]
async fn export_backup(
    Auth(caller): Reader,
    Query(params): Query<BackupParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<WithHeaders<Binary<BackupFile>, ContentDisposition>, BackupError> {
    let caller = Caller::from(caller);
    let include_secrets = params.include_secrets.unwrap_or(false);
    if include_secrets && caller.session_id().is_none() {
        return Err(BackupError::SecretsNeedSession);
    }
    let options = ExportOptions { include_secrets };
    let document = backup::export_backup(&db, caller.id(), options)
        .await
        .map_err(Internal::log)?;
    let bytes = serde_json::to_vec_pretty(&document).map_err(Internal::log)?;
    Ok(WithHeaders::new(
        Binary::new(bytes),
        attachment("stacks-backup", "json"),
    ))
}

/// Download the caller's library as CSV, one row per entry.
///
/// For spreadsheets: progress and rewatches are derived, individual plays
/// and history are left out, and the file cannot be imported back.
#[kynos::get("/exports/csv", operation_id = "exportCsv")]
async fn export_csv(
    Auth(caller): Reader,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<WithHeaders<Binary<Csv>, ContentDisposition>, ExportError> {
    let caller = Caller::from(caller);
    let csv = export::library_csv(&db, caller.id())
        .await
        .map_err(Internal::log)?;
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
