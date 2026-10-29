//! The caller's lists: manual lists they order by hand, and smart lists whose
//! titles a filter picks from the discovery feeds or the caller's library.
//!
//! A manual list's items are stored, so they render from the catalog
//! snapshot without calling a provider. A smart list's items are evaluated
//! on every read, so reading one can degrade or fail the way a catalog
//! listing does.

use std::sync::Arc;

use kynos::extract::link::Link;
use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::Patch;
use stacks_core::entity::{list, list_item};
use stacks_core::lists::fractional_index;
use stacks_core::lists::service::{CreateListRequest, Placement, UpdateListRequest};
use stacks_core::lists::{ListError as CoreError, ListService};
use stacks_core::media::{MediaError, MediaService, MetadataProvider, catalog};
use uuid::Uuid;

mod filter;

pub use filter::{ListFilter, SmartSort};

use crate::app::App;
use crate::catalog::{Degradation, TitleSummary, snapshot_summaries};
use crate::ids::TitleRef;
use crate::page::{self, InvalidCursor, PageParams};
use crate::patch::Nullable;
use crate::problem::{CatalogError, Internal, TitleNotFound, Upstream};
use crate::security::{Caller, Reader, Writer};

/// The caller's lists.
#[derive(Tag)]
#[tag(
    name = "lists",
    description = "Manual lists ordered by hand, and smart lists a filter fills."
)]
pub struct Lists;

// ------------------------------------------------------------------ DTOs

/// A list.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct List {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// Whether a filter picks the titles (`filter`) rather than the caller.
    pub smart: bool,
    /// A smart list's filter; `null` for a manual list.
    pub filter: Option<ListFilter>,
    /// How many titles a manual list holds; `null` for a smart list, whose
    /// titles are only known by reading them.
    pub item_count: Option<u64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// When the list went to the trash; `null` for a live list.
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl List {
    fn from_model(l: &list::Model, item_count: u64) -> Self {
        Self {
            id: l.id,
            name: l.name.clone(),
            description: l.description.clone(),
            smart: l.is_smart,
            filter: l.is_smart.then(|| {
                l.filter_query
                    .as_ref()
                    .map_or_else(ListFilter::default, ListFilter::from_stored)
            }),
            item_count: (!l.is_smart).then_some(item_count),
            created_at: l.created_at.to_utc(),
            updated_at: l.updated_at.to_utc(),
            deleted_at: l.deleted_at.map(|d| d.to_utc()),
        }
    }
}

/// A list to create.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct NewList {
    #[schema(min_length = 1, max_length = 200)]
    pub name: String,
    pub description: Option<String>,
    /// Makes the list a smart list, filled by this filter.
    pub filter: Option<ListFilter>,
}

/// A change to a list. Members left out are unchanged.
#[derive(Debug, Default, Schema, Deserialize)]
pub struct ListPatch {
    #[schema(min_length = 1, max_length = 200)]
    pub name: Option<String>,
    /// `null` clears the description.
    #[serde(default)]
    pub description: Nullable<String>,
    /// A smart list's filter; `null` clears its criteria. A manual list has
    /// none to change.
    #[serde(default)]
    pub filter: Nullable<ListFilter>,
}

/// One title in a list.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ListItem {
    pub title: TitleSummary,
    /// The caller's notes on the title, in this list.
    pub notes: Option<String>,
    /// Where the item sits: items sort by this string, compared bytewise.
    /// `null` for a smart list's titles, which sit in their source's order.
    pub position: Option<String>,
    /// When the title was added; `null` for a smart list's titles.
    pub added_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ListItem {
    fn stored(item: &list_item::Model, title: TitleSummary) -> Self {
        Self {
            title,
            notes: item.notes.clone(),
            position: Some(item.position.clone()),
            added_at: Some(item.created_at.to_utc()),
        }
    }

    fn evaluated(title: TitleSummary) -> Self {
        Self {
            title,
            notes: None,
            position: None,
            added_at: None,
        }
    }
}

/// One page of a list's titles.
///
/// One shape for both kinds of list: a manual list's page is never
/// degraded, and a smart list's names the provider calls that failed while
/// it was evaluated, exactly as a catalog listing does.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ListItemPage {
    pub items: Vec<ListItem>,
    /// Pass as `cursor` to fetch the next page; `null` on the last page.
    pub next_cursor: Option<String>,
    /// The provider calls that failed; the items are what the rest returned.
    pub degraded: Vec<Degradation>,
}

/// A title to add to a list.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct NewListItem {
    #[serde(rename = "ref")]
    pub reference: TitleRef,
    pub notes: Option<String>,
    /// Place it directly after this title of the list.
    pub after: Option<TitleRef>,
    /// Place it directly before this title of the list. Given with `after`,
    /// the two must be neighbours. With neither, it goes at the end.
    pub before: Option<TitleRef>,
}

/// Where to move a list item: give `after` (`null`: to the start) or
/// `before` (`null`: to the end), or both naming the same gap.
#[derive(Debug, Default, Schema, Deserialize)]
pub struct ItemPlacement {
    /// Directly after this title of the list; `null` for the start.
    #[serde(default)]
    pub after: Nullable<TitleRef>,
    /// Directly before this title of the list; `null` for the end.
    #[serde(default)]
    pub before: Nullable<TitleRef>,
}

/// A change to a list item. Members left out are unchanged; it moves only
/// when `after` or `before` is given.
//
// The placement is flattened rather than inline so serde reads this only as
// an object: a struct whose every member defaults would otherwise also
// accept a JSON array (serde's sequence form), which the schema says it is
// not, and kynos 0.4 does not check the body's JSON type
// (getkono/kynos#1002).
#[derive(Debug, Default, Schema, Deserialize)]
pub struct ListItemPatch {
    #[serde(flatten)]
    pub placement: ItemPlacement,
    /// `null` clears the notes.
    #[serde(default)]
    pub notes: Nullable<String>,
}

/// What `/lists/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct ListPath {
    pub id: Uuid,
}

/// What `/lists/{id}/items/{ref}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct ListItemPath {
    pub id: Uuid,
    /// The title's reference, such as `tmdb:movie:550`.
    #[param(rename = "ref")]
    pub reference: TitleRef,
}

/// How lists are listed.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct ListListsParams {
    /// `true` lists the trash instead of the live lists.
    pub deleted: Option<bool>,
    /// The `next_cursor` of the previous page. Omit for the first page.
    pub cursor: Option<String>,
    /// At most this many items (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u32>,
}

// ---------------------------------------------------------------- errors

/// No list of the caller's has that id (or it is in the trash, for an
/// operation on a live list).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no list of yours has that id")]
#[problem(base = crate::problem::BASE, status = 404, title = "List not found")]
pub struct ListNotFound;

/// The title named is not in the list.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{0} is not in this list")]
#[problem(base = crate::problem::BASE, status = 404, title = "List item not found")]
pub struct ListItemNotFound(pub String);

/// A list's name or filter is unusable.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{0}")]
#[problem(base = crate::problem::BASE, status = 422, title = "Invalid list")]
pub struct InvalidList(pub String);

/// The title is already in the list.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the title is already in this list")]
#[problem(base = crate::problem::BASE, status = 409, title = "Already in list")]
pub struct AlreadyInList;

/// A smart list's titles come from its filter.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("a smart list's titles come from its filter and cannot be added by hand")]
#[problem(base = crate::problem::BASE, status = 422, title = "Not a manual list")]
pub struct NotAManualList;

/// `after` and `before` do not name one gap, or name the item itself.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{0}")]
#[problem(base = crate::problem::BASE, status = 422, title = "Invalid placement")]
pub struct InvalidPlacement(pub String);

/// What listing lists can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListListsError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What creating a list can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum CreateListError {
    #[error(transparent)]
    #[problem(transparent)]
    Invalid(#[from] InvalidList),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What reading, deleting or restoring one list can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListLookupError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] ListNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What changing a list can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum UpdateListError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] ListNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Invalid(#[from] InvalidList),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What reading a list's titles can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListItemsError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] ListNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Upstream(#[from] Upstream),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What adding a title to a list can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum AddListItemError {
    #[error(transparent)]
    #[problem(transparent)]
    ListNotFound(#[from] ListNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    TitleNotFound(#[from] TitleNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    AnchorNotFound(#[from] ListItemNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    AlreadyInList(#[from] AlreadyInList),
    #[error(transparent)]
    #[problem(transparent)]
    NotAManualList(#[from] NotAManualList),
    #[error(transparent)]
    #[problem(transparent)]
    InvalidPlacement(#[from] InvalidPlacement),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What changing a list item can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum UpdateListItemError {
    #[error(transparent)]
    #[problem(transparent)]
    ListNotFound(#[from] ListNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    ItemNotFound(#[from] ListItemNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    InvalidPlacement(#[from] InvalidPlacement),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What removing a title from a list can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum RemoveListItemError {
    #[error(transparent)]
    #[problem(transparent)]
    ListNotFound(#[from] ListNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    ItemNotFound(#[from] ListItemNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// A core failure no request could have caused: logged, and a 500.
#[track_caller]
fn unexpected(e: CoreError) -> Internal {
    match e {
        CoreError::Db(e) => Internal::from(e),
        other => Internal::log(other),
    }
}

/// Another user's list is as absent as one that never existed.
fn is_missing(e: &CoreError) -> bool {
    matches!(e, CoreError::NotFound | CoreError::NotOwner)
}

impl From<CoreError> for ListLookupError {
    fn from(e: CoreError) -> Self {
        if is_missing(&e) {
            ListNotFound.into()
        } else {
            unexpected(e).into()
        }
    }
}

impl From<CoreError> for CreateListError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::EmptyName => InvalidList("the name must not be blank".into()).into(),
            CoreError::InvalidFilter(why) => InvalidList(format!("invalid filter: {why}")).into(),
            other => unexpected(other).into(),
        }
    }
}

impl From<CoreError> for UpdateListError {
    fn from(e: CoreError) -> Self {
        match e {
            e if is_missing(&e) => ListNotFound.into(),
            CoreError::EmptyName => InvalidList("the name must not be blank".into()).into(),
            CoreError::InvalidFilter(why) => InvalidList(format!("invalid filter: {why}")).into(),
            CoreError::NotSmart => InvalidList("only a smart list has a filter".into()).into(),
            other => unexpected(other).into(),
        }
    }
}

impl From<CoreError> for ListItemsError {
    fn from(e: CoreError) -> Self {
        match e {
            e if is_missing(&e) => ListNotFound.into(),
            CoreError::Media(e) => match CatalogError::from(e) {
                CatalogError::Upstream(u) => u.into(),
                CatalogError::Internal(i) => i.into(),
                // A feed has no title of its own to miss.
                CatalogError::NotFound(TitleNotFound(what)) => Internal::log(what).into(),
            },
            other => unexpected(other).into(),
        }
    }
}

impl From<CoreError> for AddListItemError {
    fn from(e: CoreError) -> Self {
        match e {
            e if is_missing(&e) => ListNotFound.into(),
            CoreError::NotManual => NotAManualList.into(),
            CoreError::AlreadyInList => AlreadyInList.into(),
            CoreError::AnchorNotFound => {
                ListItemNotFound("the title to place it next to".into()).into()
            }
            CoreError::InvalidPlacement(why) => InvalidPlacement(why.into()).into(),
            other => unexpected(other).into(),
        }
    }
}

impl From<CoreError> for UpdateListItemError {
    fn from(e: CoreError) -> Self {
        match e {
            e if is_missing(&e) => ListNotFound.into(),
            CoreError::ItemNotFound => ListItemNotFound("the title".into()).into(),
            CoreError::AnchorNotFound => {
                ListItemNotFound("the title to place it next to".into()).into()
            }
            CoreError::InvalidPlacement(why) => InvalidPlacement(why.into()).into(),
            other => unexpected(other).into(),
        }
    }
}

impl From<CoreError> for RemoveListItemError {
    fn from(e: CoreError) -> Self {
        match e {
            e if is_missing(&e) => ListNotFound.into(),
            CoreError::ItemNotFound => ListItemNotFound("the title".into()).into(),
            other => unexpected(other).into(),
        }
    }
}

// --------------------------------------------------------------- helpers

/// The catalog row a reference names, or `missing` when Stacks has never
/// stored it (so no list can hold it).
async fn stored_title<E: From<ListItemNotFound> + From<Internal>>(
    db: &DatabaseConnection,
    reference: &TitleRef,
) -> Result<uuid::Uuid, E> {
    catalog::find_by_ref(db, reference.media_id())
        .await
        .map_err(Internal::from)?
        .map(|m| m.id)
        .ok_or_else(|| ListItemNotFound(reference.to_string()).into())
}

/// A placement's anchor, resolved to its catalog row.
async fn anchor<E: From<ListItemNotFound> + From<Internal>>(
    db: &DatabaseConnection,
    side: Nullable<TitleRef>,
) -> Result<Patch<uuid::Uuid>, E> {
    Ok(match side {
        Nullable::Absent => Patch::Absent,
        Nullable::Null => Patch::Null,
        Nullable::Value(r) => Patch::Value(stored_title::<E>(db, &r).await?),
    })
}

/// Render stored items with their titles; an item whose title cannot be
/// named is left out (and logged by [`snapshot_summaries`]).
async fn render_items(
    db: &DatabaseConnection,
    caller: &Caller,
    items: Vec<(list_item::Model, stacks_core::entity::media::Model)>,
) -> Result<Vec<ListItem>, Internal> {
    let rows: Vec<_> = items.iter().map(|(_, m)| m.clone()).collect();
    let mut summaries = snapshot_summaries(db, caller.id(), &rows).await?;
    Ok(items
        .iter()
        .filter_map(|(item, m)| Some(ListItem::stored(item, summaries.remove(&m.id)?)))
        .collect())
}

/// One stored item, rendered.
async fn render_item(
    db: &DatabaseConnection,
    caller: &Caller,
    item: list_item::Model,
) -> Result<ListItem, Internal> {
    use sea_orm::EntityTrait as _;
    let row = stacks_core::entity::media::Entity::find_by_id(item.media_id)
        .one(db)
        .await?
        .ok_or_else(|| Internal::log("a list item without its catalog row"))?;
    render_items(db, caller, vec![(item, row)])
        .await?
        .pop()
        .ok_or_else(|| Internal::log("a list item whose title cannot be named"))
}

async fn render_list(svc: &ListService, l: &list::Model) -> Result<List, Internal> {
    let count = if l.is_smart {
        0
    } else {
        svc.item_count(l.id).await.map_err(unexpected)?
    };
    Ok(List::from_model(l, count))
}

// ------------------------------------------------------------ operations

/// List the caller's lists, newest first.
///
/// With `deleted=true`, the trash instead.
#[kynos::get("/lists", operation_id = "listLists")]
async fn list_lists(
    Auth(caller): Reader,
    Query(params): Query<ListListsParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<page::Page<List>>, ListListsError> {
    let caller = Caller::from(caller);
    let paging = PageParams {
        cursor: params.cursor,
        limit: params.limit,
    };
    let svc = ListService::new(db);
    let lists = if params.deleted.unwrap_or(false) {
        svc.trash(caller.id()).await
    } else {
        svc.get_user_lists(caller.id()).await
    }
    .map_err(unexpected)?;
    let page = page::by_id_desc(lists, |l| l.id, &paging)?;
    let ids: Vec<Uuid> = page.items.iter().map(|l| l.id).collect();
    let counts = svc.item_counts(&ids).await.map_err(unexpected)?;
    Ok(Json(page.map(|l| {
        List::from_model(&l, counts.get(&l.id).copied().unwrap_or(0))
    })))
}

/// Read one of the caller's lists.
#[kynos::get("/lists/{id}", operation_id = "getList")]
async fn get_list(
    Auth(caller): Reader,
    Path(path): Path<ListPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<List>, ListLookupError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db);
    let list = svc.get_list(caller.id(), path.id).await?;
    Ok(Json(render_list(&svc, &list).await?))
}

/// Create a list.
///
/// With a `filter`, a smart list that the filter fills; without, a manual
/// list.
#[kynos::post("/lists", operation_id = "createList")]
async fn create_list(
    Auth(caller): Writer,
    list_link: Link<get_list>,
    Inject(db): Inject<DatabaseConnection>,
    Json(body): Json<NewList>,
) -> Result<Created<Json<List>>, CreateListError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db);
    let is_smart = body.filter.is_some();
    let filter_query = body.filter.map(ListFilter::into_stored).transpose()?;
    let list = svc
        .create_list(
            caller.id(),
            CreateListRequest {
                name: body.name,
                description: body.description,
                is_smart,
                filter_query,
            },
        )
        .await?;
    let location = list_link.uri(ListPath { id: list.id });
    Ok(Created::at(location, Json(List::from_model(&list, 0))))
}

/// Change a list's name, description or filter.
///
/// Merge semantics: a member left out is unchanged.
#[kynos::patch("/lists/{id}", operation_id = "updateList")]
async fn update_list(
    Auth(caller): Writer,
    Path(path): Path<ListPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(patch): Json<ListPatch>,
) -> Result<Json<List>, UpdateListError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db);
    let filter_query = match patch.filter {
        Nullable::Absent => Patch::Absent,
        Nullable::Null => Patch::Null,
        Nullable::Value(f) => Patch::Value(f.into_stored()?),
    };
    let list = svc
        .update_list(
            caller.id(),
            path.id,
            UpdateListRequest {
                name: patch.name,
                description: patch.description.into_patch(),
                filter_query,
            },
        )
        .await?;
    Ok(Json(render_list(&svc, &list).await?))
}

/// Move a list to the trash.
///
/// Its titles stay with it; `restoreList` brings it back.
#[kynos::delete("/lists/{id}", operation_id = "deleteList")]
async fn delete_list(
    Auth(caller): Writer,
    Path(path): Path<ListPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, ListLookupError> {
    let caller = Caller::from(caller);
    ListService::new(db)
        .delete_list(caller.id(), path.id)
        .await?;
    Ok(NoContent)
}

/// Bring a list back from the trash.
///
/// Restoring a live list changes nothing.
#[kynos::post("/lists/{id}/restorations", operation_id = "restoreList")]
async fn restore_list(
    Auth(caller): Writer,
    Path(path): Path<ListPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<List>, ListLookupError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db);
    let list = svc.restore_list(caller.id(), path.id).await?;
    Ok(Json(render_list(&svc, &list).await?))
}

/// List a list's titles.
///
/// A manual list's in their order. A smart list's are evaluated from its
/// source on every read; drawn from a feed, a failing provider shows in
/// `degraded`, or fails the read when nothing could be fetched.
#[kynos::get("/lists/{id}/items", operation_id = "listListItems")]
async fn list_list_items(
    Auth(caller): Reader,
    Path(path): Path<ListPath>,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<Json<ListItemPage>, ListItemsError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db.clone());
    let list = svc.get_list(caller.id(), path.id).await?;
    if list.is_smart {
        // A smart list's titles have no stored order to resume after, so
        // its cursor is an offset into the evaluated feed.
        let offset: u32 = params.after()?.unwrap_or(0);
        let listing = svc
            .evaluate_smart_list(metadata.as_ref(), caller.id(), list.id)
            .await?;
        let degraded = listing.degraded.iter().map(Degradation::from).collect();
        let titles: Vec<ListItem> = listing
            .items
            .into_iter()
            .filter_map(TitleSummary::from_media)
            .map(ListItem::evaluated)
            .skip(offset as usize)
            .collect();
        let limit = params.limit();
        let page = page::take(titles, limit, |_| page::encode(&(offset + limit)));
        return Ok(Json(ListItemPage {
            items: page.items,
            next_cursor: page.next_cursor,
            degraded,
        }));
    }
    let after: Option<String> = params.after()?;
    if after
        .as_deref()
        .is_some_and(|p| !fractional_index::is_valid(p))
    {
        return Err(InvalidCursor.into());
    }
    let mut items = svc.items(caller.id(), list.id).await?;
    if let Some(after) = &after {
        items.retain(|(i, _)| i.position.as_str() > after.as_str());
    }
    let page = page::take(items, params.limit(), |(last, _)| {
        page::encode(&last.position)
    });
    Ok(Json(ListItemPage {
        items: render_items(&db, &caller, page.items).await?,
        next_cursor: page.next_cursor,
        degraded: Vec::new(),
    }))
}

/// Add a title to a manual list.
///
/// At the end, or next to a title already in the list. A title Stacks has
/// not stored yet is fetched from its provider; when the provider is down
/// it is stored as a placeholder and filled in later.
#[kynos::post("/lists/{id}/items", operation_id = "addListItem")]
async fn add_list_item(
    Auth(caller): Writer,
    Path(path): Path<ListPath>,
    items_link: Link<list_list_items>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
    Json(body): Json<NewListItem>,
) -> Result<Created<Json<ListItem>>, AddListItemError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db.clone());
    let list = svc.get_list(caller.id(), path.id).await?;
    if list.is_smart {
        return Err(NotAManualList.into());
    }
    let title = MediaService::new(db.clone(), metadata)
        .ensure_in_catalog(body.reference.media_id())
        .await
        .map_err(|e| match e {
            MediaError::NotFound(what) => AddListItemError::from(TitleNotFound(what)),
            MediaError::Db(e) => Internal::from(e).into(),
            other => Internal::log(other).into(),
        })?;
    let to = Placement {
        after: anchor::<AddListItemError>(
            &db,
            body.after.map_or(Nullable::Absent, Nullable::Value),
        )
        .await?,
        before: anchor::<AddListItemError>(
            &db,
            body.before.map_or(Nullable::Absent, Nullable::Value),
        )
        .await?,
    };
    let item = svc
        .add_item_at(caller.id(), list.id, title.id, body.notes, to)
        .await?;
    let rendered = render_items(&db, &caller, vec![(item, title)])
        .await?
        .pop()
        .ok_or_else(|| Internal::log("a list item whose title cannot be named"))?;
    let location = items_link.uri((ListPath { id: list.id }, PageParams::default()));
    Ok(Created::at(location, Json(rendered)))
}

/// Move a list item, or change its notes.
///
/// Merge semantics: a member left out is unchanged, and an item is moved
/// only when `after` or `before` is given.
#[kynos::patch("/lists/{id}/items/{ref}", operation_id = "updateListItem")]
async fn update_list_item(
    Auth(caller): Writer,
    Path(path): Path<ListItemPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(patch): Json<ListItemPatch>,
) -> Result<Json<ListItem>, UpdateListItemError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db.clone());
    let list = svc.get_list(caller.id(), path.id).await?;
    let media_id = stored_title::<UpdateListItemError>(&db, &path.reference).await?;
    let ItemPlacement { after, before } = patch.placement;
    let moving = !matches!((&after, &before), (Nullable::Absent, Nullable::Absent));
    let to = if moving {
        Some(Placement {
            after: anchor::<UpdateListItemError>(&db, after).await?,
            before: anchor::<UpdateListItemError>(&db, before).await?,
        })
    } else {
        None
    };
    let item = svc
        .update_item(caller.id(), list.id, media_id, to, patch.notes.into_patch())
        .await?;
    Ok(Json(render_item(&db, &caller, item).await?))
}

/// Remove a title from a manual list.
#[kynos::delete("/lists/{id}/items/{ref}", operation_id = "removeListItem")]
async fn remove_list_item(
    Auth(caller): Writer,
    Path(path): Path<ListItemPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, RemoveListItemError> {
    let caller = Caller::from(caller);
    let svc = ListService::new(db.clone());
    let list = svc.get_list(caller.id(), path.id).await?;
    let media_id = stored_title::<RemoveListItemError>(&db, &path.reference).await?;
    svc.remove_item(caller.id(), list.id, media_id).await?;
    Ok(NoContent)
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Lists>().mount(kynos::routes![
        list_lists,
        get_list,
        create_list,
        update_list,
        delete_list,
        restore_list,
        list_list_items,
        add_list_item,
        update_list_item,
        remove_list_item,
    ])
}
