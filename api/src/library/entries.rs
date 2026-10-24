//! Library entries and their watch runs.

use std::sync::Arc;

use kynos::extract::link::Link;
use kynos::http::Response;
use kynos::openapi::Responses as Described;
use kynos::prelude::*;
use kynos::response::{IntoResponse, Responses};
use kynos::schema::registry::Registry;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::library::{
    EntryPosition, EntryView, LibraryError, LibraryFilter, LibraryService, LibrarySort,
};
use stacks_core::media::{MediaError, MediaService, MetadataProvider, catalog};
use uuid::Uuid;

use super::{
    EntryBody, EntryChanges, EntryNotFound, EntryPath, LibraryEntry, LibraryEntryDetail,
    LibraryParams, NewRun, RunChanges, RunPath, TrashedEntryNotFound, WatchRun, WatchRunNotFound,
    WatchedEpisode, render_entries, render_entry, unexpected,
};
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::{Internal, TitleNotFound};
use crate::security::{Caller, Reader, Writer};

// ----------------------------------------------------------------- errors

/// What listing the library can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListLibraryError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What an operation on one live entry can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum EntryError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] EntryNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<LibraryError> for EntryError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::NotFound => EntryNotFound.into(),
            other => unexpected(other).into(),
        }
    }
}

/// What storing an entry can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum PutEntryError {
    #[error(transparent)]
    #[problem(transparent)]
    TitleNotFound(#[from] TitleNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What restoring an entry can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum RestoreEntryError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] TrashedEntryNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What changing a run can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum UpdateRunError {
    #[error(transparent)]
    #[problem(transparent)]
    EntryNotFound(#[from] EntryNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    RunNotFound(#[from] WatchRunNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

// ---------------------------------------------------------------- helpers

/// The catalog row of a title the caller's data refers to; absent is the
/// operation's own 404.
pub(super) async fn stored_title(
    db: &DatabaseConnection,
    reference: &crate::ids::TitleRef,
) -> Result<Option<stacks_core::entity::media::Model>, Internal> {
    Ok(catalog::find_by_ref(db, reference.media_id()).await?)
}

/// The catalog row of a title a write is adding, fetched and stored the
/// first time. Only the provider saying the title does not exist fails; an
/// outage stores a placeholder.
pub(super) async fn ensure_title(
    db: &DatabaseConnection,
    metadata: Arc<dyn MetadataProvider>,
    reference: &crate::ids::TitleRef,
) -> Result<stacks_core::entity::media::Model, PutEntryError> {
    MediaService::new(db.clone(), metadata)
        .ensure_in_catalog(reference.media_id())
        .await
        .map_err(|e| match e {
            MediaError::NotFound(what) => TitleNotFound(what).into(),
            MediaError::Db(e) => Internal::from(e).into(),
            other => Internal::log(other).into(),
        })
}

/// The caller's live entry for a stored title, as a view.
async fn live_view(
    service: &LibraryService,
    user_id: Uuid,
    media_id: Uuid,
) -> Result<Option<EntryView>, Internal> {
    service.get(user_id, media_id).await.map_err(unexpected)
}

/// Where a listing continues: the listing it belongs to (filters and order),
/// and the position of the last entry returned.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ListCursor {
    filter: LibraryFilter,
    sort: LibrarySort,
    after: EntryPosition,
}

// ------------------------------------------------------------- operations

/// List the caller's library.
///
/// Filter by status, category, favourite, tag or title text; order by when
/// entries changed (the default), were added, by title or by score. With
/// `deleted=true`, lists the trash instead.
#[kynos::get("/library", operation_id = "listLibrary")]
pub(super) async fn list_library(
    Auth(caller): Reader,
    Query(params): Query<LibraryParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<LibraryEntry>>, ListLibraryError> {
    let caller = Caller::from(caller);
    let filter = LibraryFilter {
        status: params.status.map(Into::into),
        category: params.category.map(Into::into),
        favourite: params.favourite,
        tag: params.tag,
        q: params
            .q
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_owned),
        deleted: params.deleted.unwrap_or(false),
    };
    let sort: LibrarySort = params.sort.map(Into::into).unwrap_or_default();
    let paging = PageParams {
        cursor: params.cursor,
        limit: params.limit,
    };
    let after = match paging.after::<ListCursor>()? {
        None => None,
        Some(c) if c.filter == filter && c.sort == sort && c.after.sort() == sort => Some(c.after),
        Some(_) => return Err(InvalidCursor.into()),
    };
    let service = LibraryService::new(db.clone());
    let window = service
        .list(
            caller.id(),
            &filter,
            sort,
            after.as_ref(),
            u64::from(paging.limit()),
        )
        .await
        .map_err(unexpected)?;
    let next_cursor = window
        .more
        .then(|| window.items.last())
        .flatten()
        .map(|last| {
            page::encode(&ListCursor {
                filter: filter.clone(),
                sort,
                after: EntryPosition::of(sort, last),
            })
        });
    let items = render_entries(&db, caller.id(), window.items).await?;
    Ok(Json(Page::new(items, next_cursor)))
}

/// Read one entry of the caller's library.
///
/// With its runs, and the episodes watched in the current run (what a
/// seasons grid shows).
#[kynos::get("/library/{ref}", operation_id = "getLibraryEntry")]
pub(super) async fn get_library_entry(
    Auth(caller): Reader,
    Path(path): Path<EntryPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<LibraryEntryDetail>, EntryError> {
    let caller = Caller::from(caller);
    let row = stored_title(&db, &path.reference)
        .await?
        .ok_or(EntryNotFound)?;
    let service = LibraryService::new(db.clone());
    let view = live_view(&service, caller.id(), row.id)
        .await?
        .ok_or(EntryNotFound)?;
    let runs = service.runs(caller.id(), row.id).await?;
    let watched = service.watched_in_current_run(view.entry.id).await?;
    let entry = render_entry(&db, caller.id(), view).await?;
    Ok(Json(LibraryEntryDetail {
        entry,
        runs: runs.iter().map(WatchRun::from).collect(),
        watched: watched
            .into_iter()
            .map(|w| WatchedEpisode {
                season: u32::try_from(w.season).unwrap_or_default(),
                episode: u32::try_from(w.episode).unwrap_or_default(),
                watched_at: w.watched_at.map(|t| t.to_utc()),
            })
            .collect(),
    }))
}

/// What storing an entry answers with: 201 when the title was not in the
/// library (or was in its trash), 200 when an entry was replaced.
pub enum PutEntryReply {
    Created(Created<Json<LibraryEntry>>),
    Replaced(Json<LibraryEntry>),
}

impl IntoResponse for PutEntryReply {
    fn into_response(self) -> Response {
        match self {
            Self::Created(created) => created.into_response(),
            Self::Replaced(replaced) => replaced.into_response(),
        }
    }
}

/// Both statuses, the 201 with its `Location`. `#[derive(Reply)]` cannot
/// carry a header on one variant, so the union is written out.
impl Responses for PutEntryReply {
    fn responses(registry: &mut Registry) -> Described {
        let mut responses = <Created<Json<LibraryEntry>> as Responses>::responses(registry);
        responses.merge_from(&<Json<LibraryEntry> as Responses>::responses(registry));
        responses
    }
}

/// Add a title to the library, or replace its entry.
///
/// Create-or-replace: every member is set, and one left out is cleared
/// (`favourite` to `false`). A title in the trash comes back. Answers 201
/// when the title was not in the library, 200 when its entry was replaced.
#[kynos::put("/library/{ref}", operation_id = "putLibraryEntry")]
pub(super) async fn put_library_entry(
    Auth(caller): Writer,
    Path(path): Path<EntryPath>,
    entry_link: Link<get_library_entry>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
    Json(body): Json<EntryBody>,
) -> Result<PutEntryReply, PutEntryError> {
    let caller = Caller::from(caller);
    let row = ensure_title(&db, metadata, &path.reference).await?;
    let service = LibraryService::new(db.clone());
    let (_, created) = service
        .replace(caller.id(), row.id, body.into_state())
        .await
        .map_err(unexpected)?;
    let view = live_view(&service, caller.id(), row.id)
        .await?
        .ok_or_else(|| Internal::log("an entry just stored is not live"))?;
    let entry = render_entry(&db, caller.id(), view).await?;
    Ok(if created {
        let location = entry_link.uri(EntryPath {
            reference: path.reference,
        });
        PutEntryReply::Created(Created::at(location, Json(entry)))
    } else {
        PutEntryReply::Replaced(Json(entry))
    })
}

/// Change an entry of the caller's library.
///
/// Merge semantics: a member left out is unchanged, and `null` clears
/// `score` or `notes`.
#[kynos::patch("/library/{ref}", operation_id = "updateLibraryEntry")]
pub(super) async fn update_library_entry(
    Auth(caller): Writer,
    Path(path): Path<EntryPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(changes): Json<EntryChanges>,
) -> Result<Json<LibraryEntry>, EntryError> {
    let caller = Caller::from(caller);
    let row = stored_title(&db, &path.reference)
        .await?
        .ok_or(EntryNotFound)?;
    let service = LibraryService::new(db.clone());
    service
        .patch(caller.id(), row.id, changes.into_patch())
        .await?;
    let view = live_view(&service, caller.id(), row.id)
        .await?
        .ok_or(EntryNotFound)?;
    Ok(Json(render_entry(&db, caller.id(), view).await?))
}

/// Move an entry to the trash.
///
/// Its runs, plays, tags and history are kept, and `restoreLibraryEntry`
/// brings it back exactly as it was.
#[kynos::delete("/library/{ref}", operation_id = "deleteLibraryEntry")]
pub(super) async fn delete_library_entry(
    Auth(caller): Writer,
    Path(path): Path<EntryPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, EntryError> {
    let caller = Caller::from(caller);
    let row = stored_title(&db, &path.reference)
        .await?
        .ok_or(EntryNotFound)?;
    LibraryService::new(db).delete(caller.id(), row.id).await?;
    Ok(NoContent)
}

/// Bring an entry back from the trash, exactly as it was.
#[kynos::post("/library/{ref}/restorations", operation_id = "restoreLibraryEntry")]
pub(super) async fn restore_library_entry(
    Auth(caller): Writer,
    Path(path): Path<EntryPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<LibraryEntry>, RestoreEntryError> {
    let caller = Caller::from(caller);
    let row = stored_title(&db, &path.reference)
        .await?
        .ok_or(TrashedEntryNotFound)?;
    let service = LibraryService::new(db.clone());
    service
        .restore(caller.id(), row.id)
        .await
        .map_err(|e| match e {
            LibraryError::NotFound => RestoreEntryError::from(TrashedEntryNotFound),
            other => unexpected(other).into(),
        })?;
    let view = live_view(&service, caller.id(), row.id)
        .await?
        .ok_or_else(|| Internal::log("an entry just restored is not live"))?;
    Ok(Json(render_entry(&db, caller.id(), view).await?))
}

/// Start a rewatch.
///
/// A new run, started today unless `started` says otherwise; the entry goes
/// back to `watching`.
#[kynos::post("/library/{ref}/runs", operation_id = "startRewatch")]
pub(super) async fn start_rewatch(
    Auth(caller): Writer,
    Path(path): Path<EntryPath>,
    run_link: Link<update_watch_run>,
    Inject(db): Inject<DatabaseConnection>,
    Json(body): Json<NewRun>,
) -> Result<Created<Json<WatchRun>>, EntryError> {
    let caller = Caller::from(caller);
    let row = stored_title(&db, &path.reference)
        .await?
        .ok_or(EntryNotFound)?;
    let run = LibraryService::new(db)
        .start_rewatch(caller.id(), row.id, body.started.map(Into::into))
        .await?;
    let run = WatchRun::from(&run);
    let location = run_link.uri(RunPath {
        reference: path.reference,
        number: run.number,
    });
    Ok(Created::at(location, Json(run)))
}

/// Change a run's dates.
///
/// Merge semantics: a member left out is unchanged, and `null` clears it.
#[kynos::patch("/library/{ref}/runs/{number}", operation_id = "updateWatchRun")]
pub(super) async fn update_watch_run(
    Auth(caller): Writer,
    Path(path): Path<RunPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(changes): Json<RunChanges>,
) -> Result<Json<WatchRun>, UpdateRunError> {
    let caller = Caller::from(caller);
    let row = stored_title(&db, &path.reference)
        .await?
        .ok_or(EntryNotFound)?;
    let number = i32::try_from(path.number).map_err(|_| WatchRunNotFound)?;
    let run = LibraryService::new(db)
        .update_run_by_number(caller.id(), row.id, number, changes.into_patch())
        .await
        .map_err(|e| match e {
            LibraryError::NotFound => UpdateRunError::from(EntryNotFound),
            LibraryError::RunNotFound => WatchRunNotFound.into(),
            other => unexpected(other).into(),
        })?;
    Ok(Json(WatchRun::from(&run)))
}
