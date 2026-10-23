//! Bringing a third-party export into the caller's library.
//!
//! An upload (MyAnimeList XML or JSON, a TMDB JSON list) is checked against
//! its source's parser, kept, and queued: a background job matches each row
//! to a title and applies it under the chosen conflict strategy. The import
//! and its rows can be watched while it runs.
//!
//! The parsers and the matching are issue #7's; resolving an unmatched row
//! by hand arrives with it.

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
use stacks_core::import_export::ImportExportError;
use stacks_core::import_export::import::pipeline;
use uuid::Uuid;

use crate::app::{App, Settings};
use crate::catalog::{TitleSummary, snapshot_summaries};
use crate::ids::TitleRef;
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::Internal;
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
    /// A MyAnimeList XML export (decompressed).
    MalXml,
    /// A JSON array of MyAnimeList entries.
    MalJson,
    /// A JSON array of TMDB entries.
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
    /// Replace its status, score and first run's dates with the row's.
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
            CoreOutcome::Applied => Self::Applied,
            CoreOutcome::Skipped => Self::Skipped,
            CoreOutcome::Unmatched => Self::Unmatched,
            CoreOutcome::Failed => Self::Failed,
        }
    }
}

/// One row of an import.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ImportItem {
    /// The row's position in the file, from 1.
    pub row: i32,
    pub outcome: ImportOutcome,
    /// The title the row was matched to; `null` when none was.
    pub title: Option<TitleSummary>,
    /// How sure the match is, 0–1.
    pub confidence: Option<f32>,
    /// Why the row failed, when it did.
    pub error: Option<String>,
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

/// The operation is part of the contract and not built yet.
#[derive(Debug, thiserror::Error, ApiError)]
#[error(
    "resolving an import row by hand arrives with the import matching work, \
     https://github.com/justin13888/Stacks/issues/7"
)]
#[problem(base = crate::problem::BASE, status = 501, title = "Not implemented")]
pub struct NotImplemented;

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

/// What resolving a row can answer with. Until issue #7 lands it never
/// succeeds: a row that exists is answered with `not-implemented`.
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
    NotImplemented(#[from] NotImplemented),
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
        .map(|(item, _): (import_item::Model, _)| ImportItem {
            row: item.row_number,
            outcome: item.outcome.into(),
            title: item
                .matched_media_id
                .and_then(|id| summaries.get(&id).cloned()),
            confidence: item.confidence,
            error: item.error,
        })
        .collect();
    Ok(Json(ImportDetail {
        import: Import::from(&found),
        items,
    }))
}

/// Match an import row to a title by hand.
///
/// Not built yet: issue #7 brings it. A row that exists is answered with
/// 501 `not-implemented`.
#[kynos::patch("/imports/{id}/items/{row}", operation_id = "resolveImportItem")]
async fn resolve_import_item(
    Auth(caller): Writer,
    Path(path): Path<ImportItemPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(_resolution): Json<ImportItemResolution>,
) -> ResolveImportItemError {
    let caller = Caller::from(caller);
    let found = match own_import(&db, caller.id(), path.id).await {
        Ok(found) => found,
        Err(ImportError::NotFound(e)) => return e.into(),
        Err(ImportError::Internal(e)) => return e.into(),
    };
    match pipeline::find_item(&db, found.id, path.row).await {
        Ok(Some(_)) => NotImplemented.into(),
        Ok(None) => ResolveImportItemError::ImportItemNotFound,
        Err(e) => Internal::log(e).into(),
    }
}

/// The group, with the upload's body limit on `createImport` alone: every
/// other operation keeps the default.
pub(crate) fn group(settings: &Settings) -> Group<App, Propagate, (), Cons<BodySize, ()>> {
    Group::new("/").tag::<Imports>().mount(kynos::routes![
        create_import.intercept(BodySize::new(settings.import_max_bytes)),
        list_imports,
        get_import,
        resolve_import_item,
    ])
}
