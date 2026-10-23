use salvo::prelude::*;
use stacks_core::Patch;
use stacks_core::lists::service::{
    CreateListRequest as CoreCreateListRequest, UpdateListRequest as CoreUpdateListRequest,
};
use stacks_core::lists::{ListError, ListService};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;
use crate::extractors::uuid_param;
use crate::media::dto::MediaItem;
use crate::media::{ResolveError, ensure_media, find_media};

use super::dto::*;
use super::error::*;

async fn list_to_dto(
    svc: &ListService,
    l: stacks_core::entity::list::Model,
) -> Result<ListResponse, String> {
    let item_count = svc.item_count(l.id).await.map_err(|e| e.to_string())? as u32;
    Ok(ListResponse {
        id: l.id.to_string(),
        name: l.name,
        description: l.description,
        is_smart: l.is_smart,
        item_count,
        created_at: l.created_at.to_rfc3339(),
        updated_at: l.updated_at.to_rfc3339(),
    })
}

/// List all lists owned by the authenticated user.
#[salvo::oapi::endpoint(tags("Lists"), summary = "List user lists")]
pub async fn list_lists(depot: &mut Depot) -> Result<Json<Vec<ListResponse>>, GetListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db.clone());
    let lists = svc
        .get_user_lists(user.id)
        .await
        .map_err(|e| GetListError::Internal(crate::internal!(e)))?;

    let mut responses = Vec::with_capacity(lists.len());
    for l in lists {
        responses.push(
            list_to_dto(&svc, l)
                .await
                .map_err(|e| GetListError::Internal(crate::internal!(e)))?,
        );
    }
    Ok(Json(responses))
}

/// Create a new list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Create list")]
pub async fn create_list(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ListResponse>, CreateListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: CreateListRequest = req.parse_json().await.into_internal()?;

    let svc = ListService::new(state.db.clone());
    let list = svc
        .create_list(
            user.id,
            CoreCreateListRequest {
                name: body.name,
                description: body.description,
                is_smart: body.is_smart.unwrap_or(false),
                filter_query: body.filter_query,
            },
        )
        .await
        .map_err(|e| CreateListError::Internal(crate::internal!(e)))?;

    Ok(Json(list_to_dto(&svc, list).await.map_err(|e| {
        CreateListError::Internal(crate::internal!(e))
    })?))
}

/// Get a single list by ID.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Get list")]
pub async fn get_list(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ListResponse>, GetListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = uuid_param(req, "id").ok_or(GetListError::NotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db.clone());
    let list = svc.get_list(user.id, id).await.map_err(|e| match e {
        ListError::NotFound | ListError::NotOwner => GetListError::NotFound,
        e => GetListError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(list_to_dto(&svc, list).await.map_err(|e| {
        GetListError::Internal(crate::internal!(e))
    })?))
}

/// Update list metadata. Fields left out of the body are not changed.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Update list")]
pub async fn update_list(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ListResponse>, UpdateListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = uuid_param(req, "id").ok_or(UpdateListError::NotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: UpdateListRequest = req.parse_json().await.into_internal()?;

    let svc = ListService::new(state.db.clone());
    let list = svc
        .update_list(
            user.id,
            id,
            CoreUpdateListRequest {
                name: body.name,
                description: body.description.map_or(Patch::Absent, Patch::Value),
                filter_query: body.filter_query.map_or(Patch::Absent, Patch::Value),
            },
        )
        .await
        .map_err(|e| match e {
            ListError::NotFound | ListError::NotOwner => UpdateListError::NotFound,
            e => UpdateListError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(list_to_dto(&svc, list).await.map_err(|e| {
        UpdateListError::Internal(crate::internal!(e))
    })?))
}

/// Move a list to the trash (restorable; its items are kept).
#[salvo::oapi::endpoint(tags("Lists"), summary = "Delete list")]
pub async fn delete_list(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = uuid_param(req, "id").ok_or(DeleteListError::NotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db);
    svc.delete_list(user.id, id).await.map_err(|e| match e {
        ListError::NotFound | ListError::NotOwner => DeleteListError::NotFound,
        e => DeleteListError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(()))
}

/// Get the media items of a list.
///
/// Smart lists are evaluated against their stored filter query; manual lists
/// return their stored items, rendered from the catalog snapshot, in position
/// order. Every item is overlaid with the caller's library state.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Get list items")]
pub async fn get_list_items(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<Vec<MediaItem>>, GetListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = uuid_param(req, "id").ok_or(GetListError::NotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db.clone());

    let list = svc.get_list(user.id, id).await.map_err(|e| match e {
        ListError::NotFound | ListError::NotOwner => GetListError::NotFound,
        e => GetListError::Internal(crate::internal!(e)),
    })?;

    let items = if list.is_smart {
        svc.evaluate_smart_list(&state.cameo, user.id, id).await
    } else {
        svc.list_media(user.id, id).await
    }
    .map_err(|e| GetListError::Internal(crate::internal!(e)))?;

    Ok(Json(items.into_iter().map(MediaItem::from).collect()))
}

/// Add a media item to the end of a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Add item to list")]
pub async fn add_item(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<serde_json::Value>, AddItemError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = uuid_param(req, "id").ok_or(AddItemError::ListNotFound)?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: AddItemRequest = req.parse_json().await.into_internal()?;
    let media = ensure_media(&state, &body.cameo_media_id)
        .await
        .map_err(|e| match e {
            ResolveError::NotFound => AddItemError::ListNotFound,
            ResolveError::Internal(e) => AddItemError::Internal(crate::internal!(e)),
        })?;

    let svc = ListService::new(state.db);
    let item = svc
        .add_item(user.id, id, media.id, body.notes)
        .await
        .map_err(|e| match e {
            ListError::NotFound | ListError::NotOwner => AddItemError::ListNotFound,
            ListError::AlreadyInList => AddItemError::AlreadyInList,
            e => AddItemError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(serde_json::json!({
        "list_id": item.list_id,
        "cameo_media_id": body.cameo_media_id,
        "position": item.position,
        "added_at": item.created_at.to_rfc3339(),
        "notes": item.notes,
    })))
}

/// Remove a media item from a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Remove item from list")]
pub async fn remove_item(req: &mut Request, depot: &mut Depot) -> Result<Json<()>, GetListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = uuid_param(req, "id").ok_or(GetListError::NotFound)?;
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let media = find_media(&state, &cameo_id)
        .await
        .map_err(|e| GetListError::Internal(crate::internal!(e)))?
        .ok_or(GetListError::NotFound)?;
    let svc = ListService::new(state.db);
    svc.remove_item(user.id, id, media.id)
        .await
        .map_err(|e| match e {
            ListError::NotFound | ListError::NotOwner => GetListError::NotFound,
            e => GetListError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(()))
}

/// Reorder items within a list: the items are laid out in ascending
/// `position` order, followed by any items the body does not mention.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Reorder list items")]
pub async fn reorder_items(req: &mut Request, depot: &mut Depot) -> Result<Json<()>, ReorderError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = uuid_param(req, "id")
        .ok_or_else(|| ReorderError::Internal(crate::internal!("malformed list id")))?;
    let state = depot.obtain::<AppState>().unwrap().clone();
    let mut body: ReorderRequest = req.parse_json().await.into_internal()?;
    body.items.sort_by(|a, b| a.position.total_cmp(&b.position));

    let mut order = Vec::with_capacity(body.items.len());
    for item in &body.items {
        if let Some(m) = find_media(&state, &item.cameo_media_id)
            .await
            .map_err(|e| ReorderError::Internal(crate::internal!(e)))?
        {
            order.push(m.id);
        }
    }
    ListService::new(state.db)
        .reorder(user.id, id, &order)
        .await
        .map_err(|e| ReorderError::Internal(crate::internal!(e)))?;
    Ok(Json(()))
}
