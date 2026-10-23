//! Bringing data into the caller's account.
//!
//! An upload (MyAnimeList XML or JSON, a TMDB JSON export) is checked
//! against its source's parser, kept, and queued: a background job matches
//! each row to a title and applies it under the chosen conflict strategy.
//! The import and its rows can be watched while it runs, and a row no title
//! matched with certainty keeps its candidates and is resolved by hand.
//!
//! A Stacks backup is restored synchronously, in one transaction, by
//! `restoreBackup` (`docs/backup-format.md`).

use std::sync::Arc;

use kynos::error::rejection::BodyRejection;
use kynos::extract::body::multipart::{FilePart, FromPart, MultipartForm, Part};
use kynos::extract::link::Link;
use kynos::middleware::catch_panic::Propagate;
use kynos::middleware::limits::body_size::BodySize;
use kynos::middleware::stack::Cons;
use kynos::prelude::*;
use kynos::response::codec::multipart::IntoPart;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use stacks_core::entity::enums::{
    ConflictStrategy as CoreStrategy, ImportOutcome as CoreOutcome, ImportSource as CoreSource,
    ImportStatus as CoreStatus,
};
use stacks_core::entity::{import, import_item};
use stacks_core::import_export::backup::{self, restore};
use stacks_core::import_export::import::ImportPipeline;
use stacks_core::import_export::import::matching::Candidate;
use stacks_core::import_export::import::pipeline::{self, ResolveError};
use stacks_core::import_export::{ImportEntry, ImportExportError};
use stacks_core::media::MetadataProvider;
use uuid::Uuid;

use crate::app::{App, Settings};
use crate::catalog::{Kind, TitleSummary, snapshot_summaries};
use crate::ids::TitleRef;
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::{Internal, TitleNotFound};
use crate::security::{Caller, Reader, Writer};

/// Imports into the caller's library.
#[derive(Tag)]
#[tag(
    name = "imports",
    description = "Uploading a MyAnimeList or TMDB export, and following its import row by row."
)]
pub struct Imports;

/// Where an upload came from, which says how it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportSource {
    /// MyAnimeList's own XML export (decompressed).
    MalXml,
    /// MyAnimeList API v2's anime list (one page, or an array of pages).
    MalJson,
    /// TMDB account lists: the rated, watchlist and favourite endpoints'
    /// responses, keyed by endpoint or on their own.
    TmdbJson,
}

impl From<ImportSource> for CoreSource {
    fn from(s: ImportSource) -> Self {
        match s {
            ImportSource::MalXml => Self::MalXml,
            ImportSource::MalJson => Self::MalJson,
            ImportSource::TmdbJson => Self::TmdbJson,
        }
    }
}

impl From<CoreSource> for ImportSource {
    fn from(s: CoreSource) -> Self {
        match s {
            CoreSource::MalXml => Self::MalXml,
            CoreSource::MalJson => Self::MalJson,
            CoreSource::TmdbJson => Self::TmdbJson,
        }
    }
}

/// What an import does to a title already in the library.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Leave it as it is.
    #[default]
    Skip,
    /// Replace its status, score, notes, favourite flag and first run's
    /// dates with the row's, where the row has them.
    Overwrite,
    /// Fill in only what the library lacks.
    Merge,
}

impl From<ConflictStrategy> for CoreStrategy {
    fn from(s: ConflictStrategy) -> Self {
        match s {
            ConflictStrategy::Skip => Self::Skip,
            ConflictStrategy::Overwrite => Self::Overwrite,
            ConflictStrategy::Merge => Self::Merge,
        }
    }
}

impl From<CoreStrategy> for ConflictStrategy {
    fn from(s: CoreStrategy) -> Self {
        match s {
            CoreStrategy::Skip => Self::Skip,
            CoreStrategy::Overwrite => Self::Overwrite,
            CoreStrategy::Merge => Self::Merge,
        }
    }
}

/// A text part holding one of an enum's serialized names.
fn enum_part<T: DeserializeOwned>(part: Part) -> Result<T, BodyRejection> {
    let refuse = |detail: &str| BodyRejection::Schema {
        failures: [(format!("/{}", part.name), detail.to_owned())].into(),
    };
    let text = std::str::from_utf8(&part.bytes).map_err(|_| refuse("not valid UTF-8"))?;
    serde_json::from_value(serde_json::Value::String(text.trim().to_owned()))
        .map_err(|e| refuse(&e.to_string()))
}

/// The serialized name of a unit variant, as a text part.
fn text_part<T: Serialize>(value: &T, name: &str) -> Part {
    let text = match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    };
    text.into_part(name)
}

impl FromPart for ImportSource {
    fn from_part(part: Part) -> Result<Self, BodyRejection> {
        enum_part(part)
    }
}

impl IntoPart for ImportSource {
    fn into_part(self, name: &str) -> Part {
        text_part(&self, name)
    }
}

impl FromPart for ConflictStrategy {
    fn from_part(part: Part) -> Result<Self, BodyRejection> {
        enum_part(part)
    }
}

impl IntoPart for ConflictStrategy {
    fn into_part(self, name: &str) -> Part {
        text_part(&self, name)
    }
}

/// An upload to import.
#[derive(Debug, Schema, kynos::MultipartForm)]
pub struct NewImport {
    pub source: ImportSource,
    /// `skip` when left out.
    pub conflict_strategy: Option<ConflictStrategy>,
    /// The export file itself.
    pub file: FilePart,
}

/// Where an import stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    /// Queued for the background worker.
    Pending,
    Running,
    /// Every row has an outcome.
    Completed,
    /// The import stopped; `error` says why.
    Failed,
}

impl From<CoreStatus> for ImportStatus {
    fn from(s: CoreStatus) -> Self {
        match s {
            CoreStatus::Pending => Self::Pending,
            CoreStatus::Running => Self::Running,
            CoreStatus::Completed => Self::Completed,
            CoreStatus::Failed => Self::Failed,
        }
    }
}

/// How an import's rows turned out. Filled in when the import completes.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ImportCounts {
    pub total: i32,
    pub applied: i32,
    pub skipped: i32,
    pub unmatched: i32,
    pub failed: i32,
}

/// An import.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Import {
    pub id: Uuid,
    pub source: ImportSource,
    pub status: ImportStatus,
    pub conflict_strategy: ConflictStrategy,
    /// The uploaded file's name, when the upload gave one.
    pub filename: Option<String>,
    /// The uploaded file's size in bytes.
    pub size: i64,
    pub counts: ImportCounts,
    /// Why the import failed, when it did.
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<&import::Model> for Import {
    fn from(i: &import::Model) -> Self {
        Self {
            id: i.id,
            source: i.source.into(),
            status: i.status.into(),
            conflict_strategy: i.conflict_strategy.into(),
            filename: i.original_filename.clone(),
            size: i.original_size,
            counts: ImportCounts {
                total: i.total_count,
                applied: i.applied_count,
                skipped: i.skipped_count,
                unmatched: i.unmatched_count,
                failed: i.failed_count,
            },
            error: i.error.clone(),
            created_at: i.created_at.to_utc(),
            started_at: i.started_at.map(|d| d.to_utc()),
            completed_at: i.completed_at.map(|d| d.to_utc()),
        }
    }
}

/// What became of one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOutcome {
    /// Not processed yet.
    Pending,
    /// Waiting for the metadata provider, which was unavailable; the import
    /// retries it.
    PendingRetry,
    /// Applied to the library.
    Applied,
    /// Matched, and left alone by the conflict strategy.
    Skipped,
    /// No title matched.
    Unmatched,
    /// Processing the row failed; `error` says why.
    Failed,
}

impl From<CoreOutcome> for ImportOutcome {
    fn from(o: CoreOutcome) -> Self {
        match o {
            CoreOutcome::Pending => Self::Pending,
            CoreOutcome::PendingRetry => Self::PendingRetry,
            CoreOutcome::Applied => Self::Applied,
            CoreOutcome::Skipped => Self::Skipped,
            CoreOutcome::Unmatched => Self::Unmatched,
            CoreOutcome::Failed => Self::Failed,
        }
    }
}

/// What the file says about a row, for telling which title it is.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ImportRow {
    /// The title as the file names it.
    pub title: Option<String>,
    /// The file's own id for it: `mal:30`, `tmdb:550`.
    pub source_id: Option<String>,
    pub kind: Option<Kind>,
    pub year: Option<i32>,
    pub episodes: Option<i32>,
}

impl ImportRow {
    fn from_entry(e: &ImportEntry) -> Self {
        let ids = &e.external_ids;
        let source_id = [
            ("mal", &ids.mal_id),
            ("tmdb", &ids.tmdb_id),
            ("anilist", &ids.anilist_id),
            ("imdb", &ids.imdb_id),
            ("tvdb", &ids.tvdb_id),
        ]
        .into_iter()
        .find_map(|(p, id)| id.as_ref().map(|id| format!("{p}:{id}")));
        Self {
            title: e.title.clone(),
            source_id,
            kind: e.kind.map(Kind::from),
            year: e.year,
            episodes: e.total_episodes,
        }
    }
}

/// A title an unmatched row might be.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ImportCandidate {
    pub title: TitleRef,
    pub name: String,
    pub kind: Kind,
    pub year: Option<i32>,
    /// How sure the match would be, 0–1.
    #[schema(minimum = 0, maximum = 1)]
    pub confidence: f32,
}

/// One row of an import.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ImportItem {
    /// The row's position in the file, from 1.
    pub row: i32,
    pub outcome: ImportOutcome,
    /// What the file says about the row; `null` for a row that could not be
    /// read back.
    pub source: Option<ImportRow>,
    /// The title the row was matched to; `null` when none was.
    pub title: Option<TitleSummary>,
    /// How sure the match is, 0–1.
    pub confidence: Option<f32>,
    /// For an unmatched row, the titles it might be, best first; resolve it
    /// with `resolveImportItem`.
    pub candidates: Vec<ImportCandidate>,
    /// Why the row failed, or what a retrying row waits for.
    pub error: Option<String>,
}

impl ImportItem {
    fn new(item: import_item::Model, title: Option<TitleSummary>) -> Self {
        let candidates: Vec<Candidate> = item
            .candidates
            .clone()
            .and_then(|c| serde_json::from_value(c).ok())
            .unwrap_or_default();
        Self {
            row: item.row_number,
            outcome: item.outcome.into(),
            source: pipeline::row_of(&item)
                .ok()
                .map(|e| ImportRow::from_entry(&e)),
            title,
            confidence: item.confidence,
            candidates: candidates
                .into_iter()
                .filter_map(|c| {
                    Some(ImportCandidate {
                        title: c.reference.parse().ok()?,
                        name: c.title,
                        kind: c.kind.into(),
                        year: c.year,
                        confidence: c.confidence,
                    })
                })
                .collect(),
            error: item.error,
        }
    }
}

/// An import with its rows, in file order.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ImportDetail {
    #[serde(flatten)]
    pub import: Import,
    /// Empty until the worker has read the file.
    pub items: Vec<ImportItem>,
}

/// Where a new import can be followed.
#[derive(Debug, HeaderParams)]
pub struct ImportLocation {
    /// The new import's `getImport` URI.
    #[header(rename = "Location")]
    pub location: String,
}

/// What `/imports/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct ImportPath {
    pub id: Uuid,
}

/// What `/imports/{id}/items/{row}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct ImportItemPath {
    pub id: Uuid,
    /// The row's position in the file, from 1.
    #[schema(minimum = 1)]
    pub row: i32,
}

/// A row's match, chosen by hand.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct ImportItemResolution {
    /// The title the row is.
    pub title: TitleRef,
}

/// No import of the caller's has that id.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no import of yours has that id")]
#[problem(base = crate::problem::BASE, status = 404, title = "Import not found")]
pub struct ImportNotFound;

/// What uploading an import can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum CreateImportError {
    /// The file is not what its `source` says: the source's parser refused it.
    #[error("{0}")]
    #[problem(status = 422, title = "Unreadable import")]
    UnreadableImport(String),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What listing imports can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListImportsError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What reading an import can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ImportError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] ImportNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What resolving a row can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum ResolveImportItemError {
    #[error(transparent)]
    #[problem(transparent)]
    ImportNotFound(#[from] ImportNotFound),
    /// The import has no row with that number.
    #[error("the import has no row with that number")]
    #[problem(status = 404, title = "Import item not found")]
    ImportItemNotFound,
    #[error(transparent)]
    #[problem(transparent)]
    TitleNotFound(#[from] TitleNotFound),
    /// The row already has its title (it was applied or skipped), or the
    /// import has not read it yet.
    #[error("{0}")]
    #[problem(status = 409, title = "Import item not resolvable")]
    ImportItemNotResolvable(String),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// Upload an export to import into the caller's library.
///
/// The file is checked against its source's parser and queued; the answer
/// is the queued import, which `Location` names. Uploads are capped at the
/// deployment's import size limit.
#[kynos::post("/imports", operation_id = "createImport")]
async fn create_import(
    Auth(caller): Writer,
    import_link: Link<get_import>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(settings): Inject<Arc<Settings>>,
    MultipartForm(upload): MultipartForm<NewImport>,
) -> Result<WithHeaders<Accepted<Json<Import>>, ImportLocation>, CreateImportError> {
    let caller = Caller::from(caller);
    let row = pipeline::submit(
        &db,
        caller.id(),
        upload.source.into(),
        upload.file.file_name,
        upload.file.bytes.to_vec(),
        upload.conflict_strategy.unwrap_or_default().into(),
        settings.import_max_bytes,
    )
    .await
    .map_err(|e| match e {
        ImportExportError::ParseError(why) | ImportExportError::UnsupportedFormat(why) => {
            CreateImportError::UnreadableImport(why)
        }
        // The body limit is the same cap and is enforced first, so a file
        // past it never reaches here.
        other => Internal::log(other).into(),
    })?;
    let location = import_link.uri(ImportPath { id: row.id }).to_string();
    Ok(WithHeaders::new(
        Accepted::new(Json(Import::from(&row))),
        ImportLocation { location },
    ))
}

/// List the caller's imports, newest first.
#[kynos::get("/imports", operation_id = "listImports")]
async fn list_imports(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<Import>>, ListImportsError> {
    let caller = Caller::from(caller);
    let before: Option<Uuid> = params.after()?;
    let rows = pipeline::list_for_user(&db, caller.id(), before, u64::from(params.limit()) + 1)
        .await
        .map_err(Internal::log)?;
    let page = page::take(rows, params.limit(), |i| page::encode(&i.id));
    Ok(Json(page.map(|i| Import::from(&i))))
}

async fn own_import(
    db: &DatabaseConnection,
    user_id: Uuid,
    import_id: Uuid,
) -> Result<import::Model, ImportError> {
    pipeline::find_for_user(db, user_id, import_id)
        .await
        .map_err(Internal::log)?
        .ok_or_else(|| ImportNotFound.into())
}

/// Read one of the caller's imports, with every row and what became of it.
#[kynos::get("/imports/{id}", operation_id = "getImport")]
async fn get_import(
    Auth(caller): Reader,
    Path(path): Path<ImportPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<ImportDetail>, ImportError> {
    let caller = Caller::from(caller);
    let found = own_import(&db, caller.id(), path.id).await?;
    let rows = pipeline::items_with_matches(&db, found.id)
        .await
        .map_err(Internal::log)?;
    let matched: Vec<_> = rows.iter().filter_map(|(_, m)| m.clone()).collect();
    let summaries = snapshot_summaries(&db, caller.id(), &matched).await?;
    let items = rows
        .into_iter()
        .map(|(item, _): (import_item::Model, _)| {
            let title = item
                .matched_media_id
                .and_then(|id| summaries.get(&id).cloned());
            ImportItem::new(item, title)
        })
        .collect();
    Ok(Json(ImportDetail {
        import: Import::from(&found),
        items,
    }))
}

/// Match an unmatched, failed or retrying import row to a title by hand.
///
/// The row is applied to the library under the import's conflict strategy,
/// and its MyAnimeList id, when it has one, is linked to the title so the
/// next import of it matches on its own. The answer is the row as it now
/// stands.
#[kynos::patch("/imports/{id}/items/{row}", operation_id = "resolveImportItem")]
async fn resolve_import_item(
    Auth(caller): Writer,
    Path(path): Path<ImportItemPath>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
    Json(resolution): Json<ImportItemResolution>,
) -> Result<Json<ImportItem>, ResolveImportItemError> {
    let caller = Caller::from(caller);
    let found = own_import(&db, caller.id(), path.id)
        .await
        .map_err(|e| match e {
            ImportError::NotFound(e) => ResolveImportItemError::from(e),
            ImportError::Internal(e) => e.into(),
        })?;
    let (item, media) = ImportPipeline::new(db.clone(), metadata)
        .resolve_item(found.id, path.row, resolution.title.media_id())
        .await
        .map_err(|e| match e {
            ResolveError::NoSuchRow => ResolveImportItemError::ImportItemNotFound,
            e @ ResolveError::NotResolvable(_) => {
                ResolveImportItemError::ImportItemNotResolvable(e.to_string())
            }
            ResolveError::TitleNotFound(why) => TitleNotFound(why).into(),
            ResolveError::Other(e) => Internal::log(e).into(),
        })?;
    let summaries = snapshot_summaries(&db, caller.id(), std::slice::from_ref(&media)).await?;
    Ok(Json(ImportItem::new(
        item,
        summaries.get(&media.id).cloned(),
    )))
}

/// What a restore does to what the caller already has.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue,
)]
#[serde(rename_all = "snake_case")]
pub enum RestoreMode {
    /// Fill in only what is missing; add plays and events the library lacks.
    #[default]
    Merge,
    /// Leave existing entries, lists, tags and channels untouched; add what
    /// is missing.
    Skip,
    /// Let the backup's values win. Plays and events are still only added:
    /// nothing is deleted or un-voided.
    Overwrite,
}

impl From<RestoreMode> for backup::RestoreMode {
    fn from(m: RestoreMode) -> Self {
        match m {
            RestoreMode::Merge => Self::Merge,
            RestoreMode::Skip => Self::Skip,
            RestoreMode::Overwrite => Self::Overwrite,
        }
    }
}

impl From<backup::RestoreMode> for RestoreMode {
    fn from(m: backup::RestoreMode) -> Self {
        match m {
            backup::RestoreMode::Merge => Self::Merge,
            backup::RestoreMode::Skip => Self::Skip,
            backup::RestoreMode::Overwrite => Self::Overwrite,
        }
    }
}

/// How a restore treats what exists, and whether it keeps its work.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct RestoreParams {
    /// `merge` when left out.
    pub mode: Option<RestoreMode>,
    /// Do the whole restore and report it, then keep none of it.
    pub dry_run: Option<bool>,
}

/// A backup file to restore.
#[derive(Debug, Schema, kynos::MultipartForm)]
pub struct BackupUpload {
    /// A Stacks backup document, as `exportBackup` writes it.
    pub file: FilePart,
}

/// How the rows of one kind fared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RestoreTally {
    /// Added.
    pub inserted: u32,
    /// Existed, and changed.
    pub updated: u32,
    /// Existed, and already held what the backup holds.
    pub unchanged: u32,
    /// Left alone: by `skip`, or because they could not be restored (see
    /// `warnings`).
    pub skipped: u32,
}

impl From<restore::Tally> for RestoreTally {
    fn from(t: restore::Tally) -> Self {
        Self {
            inserted: t.inserted,
            updated: t.updated,
            unchanged: t.unchanged,
            skipped: t.skipped,
        }
    }
}

/// How the backup's titles were found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RestoredTitles {
    /// Already in this server's catalog.
    pub matched: u32,
    /// Added to the catalog; their metadata is fetched in the background.
    pub created: u32,
}

/// What happened to one library entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreAction {
    Inserted,
    Updated,
    Unchanged,
    Skipped,
}

/// One library entry's outcome.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RestoredEntry {
    /// The title's public id.
    pub title: String,
    pub action: RestoreAction,
}

/// What a restore did, or with `dry_run`, would do.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct RestoreReport {
    /// The format version the backup was written in; an older one was
    /// upgraded.
    pub read_version: u32,
    pub mode: RestoreMode,
    /// `true` when nothing was kept.
    pub dry_run: bool,
    pub settings_changed: bool,
    pub titles: RestoredTitles,
    pub entries: RestoreTally,
    pub runs: RestoreTally,
    pub plays: RestoreTally,
    pub events: RestoreTally,
    pub entry_tags: RestoreTally,
    pub lists: RestoreTally,
    pub list_items: RestoreTally,
    pub tags: RestoreTally,
    pub blocklist: RestoreTally,
    pub notification_channels: RestoreTally,
    pub notification_subscriptions: RestoreTally,
    /// Each library entry in the backup, in its order.
    pub changes: Vec<RestoredEntry>,
    /// What could not be restored as it was, and why.
    pub warnings: Vec<String>,
}

impl From<backup::RestoreReport> for RestoreReport {
    fn from(r: backup::RestoreReport) -> Self {
        Self {
            read_version: r.read_version,
            mode: r.mode.into(),
            dry_run: r.dry_run,
            settings_changed: r.settings_changed,
            titles: RestoredTitles {
                matched: r.titles.matched,
                created: r.titles.created,
            },
            entries: r.entries.into(),
            runs: r.runs.into(),
            plays: r.plays.into(),
            events: r.events.into(),
            entry_tags: r.entry_tags.into(),
            lists: r.lists.into(),
            list_items: r.list_items.into(),
            tags: r.tags.into(),
            blocklist: r.blocklist.into(),
            notification_channels: r.notification_channels.into(),
            notification_subscriptions: r.notification_subscriptions.into(),
            changes: r
                .changes
                .into_iter()
                .map(|c| RestoredEntry {
                    title: c.title,
                    action: match c.action {
                        restore::Action::Inserted => RestoreAction::Inserted,
                        restore::Action::Updated => RestoreAction::Updated,
                        restore::Action::Unchanged => RestoreAction::Unchanged,
                        restore::Action::Skipped => RestoreAction::Skipped,
                    },
                })
                .collect(),
            warnings: r.warnings,
        }
    }
}

/// What restoring a backup can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum RestoreBackupError {
    /// The file is not a Stacks backup: not JSON, no `format`, or not a
    /// valid document of its version.
    #[error("{0}")]
    #[problem(status = 422, title = "Unreadable backup")]
    UnreadableBackup(String),
    /// The backup was written by a newer Stacks, in a format version this
    /// server cannot read.
    #[error("{0}")]
    #[problem(status = 422, title = "Unsupported backup version")]
    UnsupportedBackupVersion(String),
    /// The backup reads, but contradicts itself (a reference to a title it
    /// does not hold, a score out of range).
    #[error("{0}")]
    #[problem(status = 422, title = "Invalid backup")]
    InvalidBackup(String),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// Restore a backup into the caller's account.
///
/// Everything is applied in one transaction, or nothing is. Rows keep the
/// ids they had; `mode` says what happens to what the caller already has,
/// and `dry_run` reports the restore without keeping it. Uploads are capped
/// at the deployment's import size limit.
#[kynos::post("/imports/backup", operation_id = "restoreBackup")]
async fn restore_backup(
    Auth(caller): Writer,
    Query(params): Query<RestoreParams>,
    Inject(db): Inject<DatabaseConnection>,
    MultipartForm(upload): MultipartForm<BackupUpload>,
) -> Result<Json<RestoreReport>, RestoreBackupError> {
    let caller = Caller::from(caller);
    let options = backup::RestoreOptions {
        mode: params.mode.unwrap_or_default().into(),
        dry_run: params.dry_run.unwrap_or(false),
    };
    let report = backup::restore(&db, caller.id(), &upload.file.bytes, options)
        .await
        .map_err(|e| match e {
            backup::RestoreError::Read(e @ backup::ReadError::UnsupportedVersion { .. }) => {
                RestoreBackupError::UnsupportedBackupVersion(e.to_string())
            }
            backup::RestoreError::Read(e) => RestoreBackupError::UnreadableBackup(e.to_string()),
            backup::RestoreError::Invalid(why) => RestoreBackupError::InvalidBackup(why),
            backup::RestoreError::Db(e) => Internal::log(e).into(),
        })?;
    Ok(Json(report.into()))
}

/// The group, with the uploads' body limit on `createImport` and
/// `restoreBackup` alone: every other operation keeps the default.
pub(crate) fn group(
    settings: &Settings,
) -> Group<App, Propagate, (), Cons<BodySize, Cons<BodySize, ()>>> {
    Group::new("/").tag::<Imports>().mount(kynos::routes![
        create_import.intercept(BodySize::new(settings.import_max_bytes)),
        restore_backup.intercept(BodySize::new(settings.import_max_bytes)),
        list_imports,
        get_import,
        resolve_import_item,
    ])
}
