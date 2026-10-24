//! The caller's tags, and putting them on titles in their library.
//!
//! A tag is a label on library entries. Deleting one takes it off every
//! entry; the entries themselves are untouched.

use kynos::extract::link::Link;
use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::entity::tag;
use stacks_core::media::catalog;
use stacks_core::tags::{TagError as CoreError, TagService};
use uuid::Uuid;

use crate::app::App;
use crate::ids::TitleRef;
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::patch::Nullable;
use crate::problem::Internal;
use crate::security::{Caller, Reader, Writer};

/// The caller's tags.
#[derive(Tag)]
#[tag(
    name = "tags",
    description = "Labels the caller puts on titles in their library."
)]
pub struct Tags;

/// A tag.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct Tag {
    pub id: Uuid,
    pub name: String,
    /// `#RRGGBB`, or `null` for none.
    pub color: Option<String>,
    /// How many titles in the caller's library carry it.
    pub entry_count: u64,
}

impl Tag {
    pub(crate) fn from_model(t: &tag::Model, entry_count: u64) -> Self {
        Self {
            id: t.id,
            name: t.name.clone(),
            color: t.color.clone(),
            entry_count,
        }
    }
}

/// A tag to create.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct NewTag {
    /// Unique among the caller's tags.
    #[schema(min_length = 1, max_length = 100)]
    pub name: String,
    /// `#RRGGBB`.
    pub color: Option<String>,
}

/// A change to a tag. Members left out are unchanged.
#[derive(Debug, Default, Schema, Deserialize)]
pub struct TagPatch {
    #[schema(min_length = 1, max_length = 100)]
    pub name: Option<String>,
    /// `#RRGGBB`; `null` clears the colour.
    #[serde(default)]
    pub color: Nullable<String>,
}

/// What `/tags/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct TagPath {
    pub id: Uuid,
}

/// What `/library/{ref}/tags/{id}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct EntryTagPath {
    /// The title's reference, such as `tmdb:movie:550`.
    #[param(rename = "ref")]
    pub reference: TitleRef,
    /// The tag's id.
    pub id: Uuid,
}

// ---------------------------------------------------------------- errors

/// No tag of the caller's has that id.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no tag of yours has that id")]
#[problem(base = crate::problem::BASE, status = 404, title = "Tag not found")]
pub struct TagNotFound;

/// The caller already has a tag of that name.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("you already have a tag with that name")]
#[problem(base = crate::problem::BASE, status = 409, title = "Tag name taken")]
pub struct TagNameTaken;

/// A tag's name or colour is unusable.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{0}")]
#[problem(base = crate::problem::BASE, status = 422, title = "Invalid tag")]
pub struct InvalidTag(pub String);

/// The title is not in the caller's library (or is in its trash).
#[derive(Debug, thiserror::Error, ApiError)]
#[error("{0} is not in your library")]
#[problem(base = crate::problem::BASE, status = 404, title = "Entry not found")]
pub struct EntryNotFound(pub String);

/// What listing tags can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListTagsError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What reading or deleting one tag can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum TagLookupError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] TagNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What creating a tag can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum CreateTagError {
    #[error(transparent)]
    #[problem(transparent)]
    Invalid(#[from] InvalidTag),
    #[error(transparent)]
    #[problem(transparent)]
    NameTaken(#[from] TagNameTaken),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What changing a tag can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum UpdateTagError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] TagNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Invalid(#[from] InvalidTag),
    #[error(transparent)]
    #[problem(transparent)]
    NameTaken(#[from] TagNameTaken),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What tagging or untagging a title can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum TagEntryError {
    #[error(transparent)]
    #[problem(transparent)]
    TagNotFound(#[from] TagNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    EntryNotFound(#[from] EntryNotFound),
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

fn invalid(e: &CoreError) -> Option<InvalidTag> {
    match e {
        CoreError::EmptyName => Some(InvalidTag("the name must not be blank".into())),
        CoreError::InvalidColor => Some(InvalidTag("a colour is `#RRGGBB`".into())),
        _ => None,
    }
}

impl From<CoreError> for TagLookupError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::NotFound => TagNotFound.into(),
            other => unexpected(other).into(),
        }
    }
}

impl From<CoreError> for CreateTagError {
    fn from(e: CoreError) -> Self {
        if let Some(i) = invalid(&e) {
            return i.into();
        }
        match e {
            CoreError::NameTaken => TagNameTaken.into(),
            other => unexpected(other).into(),
        }
    }
}

impl From<CoreError> for UpdateTagError {
    fn from(e: CoreError) -> Self {
        if let Some(i) = invalid(&e) {
            return i.into();
        }
        match e {
            CoreError::NotFound => TagNotFound.into(),
            CoreError::NameTaken => TagNameTaken.into(),
            other => unexpected(other).into(),
        }
    }
}

/// `reference`'s catalog row, which every title in a library has.
async fn tracked_title(
    db: &DatabaseConnection,
    reference: &TitleRef,
) -> Result<Uuid, TagEntryError> {
    catalog::find_by_ref(db, reference.media_id())
        .await
        .map_err(Internal::from)?
        .map(|m| m.id)
        .ok_or_else(|| EntryNotFound(reference.to_string()).into())
}

async fn render(svc: &TagService, t: &tag::Model) -> Result<Tag, Internal> {
    let counts = svc.entry_counts(&[t.id]).await.map_err(unexpected)?;
    Ok(Tag::from_model(t, counts.get(&t.id).copied().unwrap_or(0)))
}

// ------------------------------------------------------------ operations

/// List the caller's tags, newest first.
#[kynos::get("/tags", operation_id = "listTags")]
async fn list_tags(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<Tag>>, ListTagsError> {
    let caller = Caller::from(caller);
    let svc = TagService::new(db);
    let tags = svc.list_tags(caller.id()).await.map_err(unexpected)?;
    let page = page::by_id_desc(tags, |t| t.id, &params)?;
    let ids: Vec<Uuid> = page.items.iter().map(|t| t.id).collect();
    let counts = svc.entry_counts(&ids).await.map_err(unexpected)?;
    Ok(Json(page.map(|t| {
        Tag::from_model(&t, counts.get(&t.id).copied().unwrap_or(0))
    })))
}

/// Read one of the caller's tags.
#[kynos::get("/tags/{id}", operation_id = "getTag")]
async fn get_tag(
    Auth(caller): Reader,
    Path(path): Path<TagPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Tag>, TagLookupError> {
    let caller = Caller::from(caller);
    let svc = TagService::new(db);
    let tag = svc.get_tag(caller.id(), path.id).await?;
    Ok(Json(render(&svc, &tag).await?))
}

/// Create a tag.
#[kynos::post("/tags", operation_id = "createTag")]
async fn create_tag(
    Auth(caller): Writer,
    tag_link: Link<get_tag>,
    Inject(db): Inject<DatabaseConnection>,
    Json(body): Json<NewTag>,
) -> Result<Created<Json<Tag>>, CreateTagError> {
    let caller = Caller::from(caller);
    let tag = TagService::new(db)
        .create_tag(caller.id(), body.name, body.color)
        .await?;
    let location = tag_link.uri(TagPath { id: tag.id });
    Ok(Created::at(location, Json(Tag::from_model(&tag, 0))))
}

/// Rename a tag or change its colour.
///
/// Merge semantics: a member left out is unchanged.
#[kynos::patch("/tags/{id}", operation_id = "updateTag")]
async fn update_tag(
    Auth(caller): Writer,
    Path(path): Path<TagPath>,
    Inject(db): Inject<DatabaseConnection>,
    Json(patch): Json<TagPatch>,
) -> Result<Json<Tag>, UpdateTagError> {
    let caller = Caller::from(caller);
    let svc = TagService::new(db);
    let tag = svc
        .update_tag(caller.id(), path.id, patch.name, patch.color.into_patch())
        .await?;
    Ok(Json(render(&svc, &tag).await?))
}

/// Delete a tag.
///
/// It comes off every title it was on; the titles stay in the library.
#[kynos::delete("/tags/{id}", operation_id = "deleteTag")]
async fn delete_tag(
    Auth(caller): Writer,
    Path(path): Path<TagPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, TagLookupError> {
    let caller = Caller::from(caller);
    TagService::new(db).delete_tag(caller.id(), path.id).await?;
    Ok(NoContent)
}

/// Put a tag on a title in the caller's library.
///
/// Tagging a title that already carries the tag changes nothing.
#[kynos::put("/library/{ref}/tags/{id}", operation_id = "tagEntry")]
async fn tag_entry(
    Auth(caller): Writer,
    Path(path): Path<EntryTagPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, TagEntryError> {
    let caller = Caller::from(caller);
    let svc = TagService::new(db.clone());
    svc.get_tag(caller.id(), path.id)
        .await
        .map_err(entry_error(&path.reference))?;
    let media_id = tracked_title(&db, &path.reference).await?;
    svc.assign_tag(caller.id(), media_id, path.id)
        .await
        .map_err(entry_error(&path.reference))?;
    Ok(NoContent)
}

/// Take a tag off a title in the caller's library.
///
/// Untagging a title that does not carry the tag changes nothing.
#[kynos::delete("/library/{ref}/tags/{id}", operation_id = "untagEntry")]
async fn untag_entry(
    Auth(caller): Writer,
    Path(path): Path<EntryTagPath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, TagEntryError> {
    let caller = Caller::from(caller);
    let svc = TagService::new(db.clone());
    svc.get_tag(caller.id(), path.id)
        .await
        .map_err(entry_error(&path.reference))?;
    let media_id = tracked_title(&db, &path.reference).await?;
    svc.unassign_tag(caller.id(), media_id, path.id)
        .await
        .map_err(entry_error(&path.reference))?;
    Ok(NoContent)
}

/// How a core failure of tagging `reference` answers.
fn entry_error(reference: &TitleRef) -> impl Fn(CoreError) -> TagEntryError + '_ {
    move |e| match e {
        CoreError::NotFound => TagNotFound.into(),
        CoreError::NotTracked => EntryNotFound(reference.to_string()).into(),
        other => unexpected(other).into(),
    }
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Tags>().mount(kynos::routes![
        list_tags,
        get_tag,
        create_tag,
        update_tag,
        delete_tag,
        tag_entry,
        untag_entry,
    ])
}
