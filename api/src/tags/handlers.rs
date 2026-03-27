use salvo::prelude::*;
use stacks_core::tags::{TagError, TagService};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;

use super::dto::*;
use super::error::*;

fn tag_to_dto(t: stacks_core::entity::user_tag::Model) -> TagResponse {
    TagResponse {
        id: t.id,
        name: t.name,
        color: t.color,
    }
}

/// List all tags belonging to the authenticated user.
#[salvo::oapi::endpoint(tags("Tags"), summary = "List tags")]
pub async fn list_tags(depot: &mut Depot) -> Result<Json<Vec<TagResponse>>, DeleteTagError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = TagService::new(state.db);
    let tags = svc
        .list_tags(&user.id)
        .await
        .map_err(|e| DeleteTagError::Internal(crate::internal!(e)))?;
    Ok(Json(tags.into_iter().map(tag_to_dto).collect()))
}

/// Create a new tag.
#[salvo::oapi::endpoint(tags("Tags"), summary = "Create tag")]
pub async fn create_tag(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<TagResponse>, CreateTagError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: CreateTagRequest = req.parse_json().await.into_internal()?;

    let svc = TagService::new(state.db);
    let tag = svc
        .create_tag(&user.id, body.name, body.color)
        .await
        .map_err(|e| match e {
            TagError::NameTaken => CreateTagError::NameTaken,
            e => CreateTagError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(tag_to_dto(tag)))
}

/// Delete a tag by ID.
#[salvo::oapi::endpoint(tags("Tags"), summary = "Delete tag")]
pub async fn delete_tag(req: &mut Request, depot: &mut Depot) -> Result<Json<()>, DeleteTagError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = TagService::new(state.db);
    svc.delete_tag(&user.id, &id).await.map_err(|e| match e {
        TagError::NotFound => DeleteTagError::NotFound,
        e => DeleteTagError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(()))
}

/// Replace the full set of tags on a media item.
#[salvo::oapi::endpoint(tags("Tags"), summary = "Set media tags")]
pub async fn set_media_tags(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<Vec<TagResponse>>, SetMediaTagsError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: SetMediaTagsRequest = req.parse_json().await.into_internal()?;

    let svc = TagService::new(state.db);
    let tags = svc
        .set_tags_for_media(&user.id, &cameo_id, body.tag_ids)
        .await
        .map_err(|e| SetMediaTagsError::Internal(crate::internal!(e)))?;
    Ok(Json(tags.into_iter().map(tag_to_dto).collect()))
}
