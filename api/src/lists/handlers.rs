use salvo::prelude::*;
use stacks_core::lists::service::{
    CreateListRequest as CoreCreateListRequest, ReorderItem,
    UpdateListRequest as CoreUpdateListRequest,
};
use stacks_core::lists::{ListError, ListService};
use stacks_core::media::{MediaId, MediaService};

use crate::app::AppState;
use crate::error::IntoInternal;
use crate::extractors::auth_user::require_user;
use crate::media::dto::MediaItem;

use super::dto::*;
use super::error::*;

async fn list_item_count(db: &sea_orm::DatabaseConnection, list_id: &str) -> u32 {
    stacks_core::lists::repo::count_items_for_list(db, list_id)
        .await
        .unwrap_or(0) as u32
}

fn list_to_dto(l: stacks_core::entity::user_list::Model, item_count: u32) -> ListResponse {
    ListResponse {
        id: l.id,
        name: l.name,
        description: l.description,
        is_smart: l.is_smart,
        item_count,
        created_at: l.created_at.to_rfc3339(),
        updated_at: l.updated_at.to_rfc3339(),
    }
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
        .get_user_lists(&user.id)
        .await
        .map_err(|e| GetListError::Internal(crate::internal!(e)))?;

    let mut responses = Vec::with_capacity(lists.len());
    for l in lists {
        let count = list_item_count(&state.db, &l.id).await;
        responses.push(list_to_dto(l, count));
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
            &user.id,
            CoreCreateListRequest {
                name: body.name,
                description: body.description,
                is_smart: body.is_smart.unwrap_or(false),
                filter_query: body.filter_query,
            },
        )
        .await
        .map_err(|e| CreateListError::Internal(crate::internal!(e)))?;

    let count = list_item_count(&state.db, &list.id).await;
    Ok(Json(list_to_dto(list, count)))
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
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db.clone());
    let list = svc.get_list(&user.id, &id).await.map_err(|e| match e {
        ListError::NotFound | ListError::NotOwner => GetListError::NotFound,
        e => GetListError::Internal(crate::internal!(e)),
    })?;

    let count = list_item_count(&state.db, &list.id).await;
    Ok(Json(list_to_dto(list, count)))
}

/// Update list metadata.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Update list")]
pub async fn update_list(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<ListResponse>, UpdateListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: UpdateListRequest = req.parse_json().await.into_internal()?;

    let svc = ListService::new(state.db.clone());
    let list = svc
        .update_list(
            &user.id,
            &id,
            CoreUpdateListRequest {
                name: body.name,
                description: body.description,
                filter_query: body.filter_query,
            },
        )
        .await
        .map_err(|e| match e {
            ListError::NotFound | ListError::NotOwner => UpdateListError::NotFound,
            e => UpdateListError::Internal(crate::internal!(e)),
        })?;

    let count = list_item_count(&state.db, &list.id).await;
    Ok(Json(list_to_dto(list, count)))
}

/// Delete a list by ID.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Delete list")]
pub async fn delete_list(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<()>, DeleteListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db);
    svc.delete_list(&user.id, &id).await.map_err(|e| match e {
        ListError::NotFound | ListError::NotOwner => DeleteListError::NotFound,
        e => DeleteListError::Internal(crate::internal!(e)),
    })?;
    Ok(Json(()))
}

/// Get the media items of a list.
///
/// Smart lists are evaluated against their stored filter query; manual lists
/// return their stored items, resolved to full media metadata, in position
/// order. Every item is overlaid with the caller's watch state.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Get list items")]
pub async fn get_list_items(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<Vec<MediaItem>>, GetListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db.clone());

    // Resolve the list and confirm ownership before doing any cameo work.
    let list = svc.get_list(&user.id, &id).await.map_err(|e| match e {
        ListError::NotFound | ListError::NotOwner => GetListError::NotFound,
        e => GetListError::Internal(crate::internal!(e)),
    })?;

    let items = if list.is_smart {
        svc.evaluate_smart_list(&state.cameo, &user.id, &id)
            .await
            .map_err(|e| GetListError::Internal(crate::internal!(e)))?
    } else {
        let stored = stacks_core::lists::repo::find_items_for_list(&state.db, &id)
            .await
            .map_err(|e| GetListError::Internal(crate::internal!(e)))?;
        let ids: Vec<MediaId> = stored
            .iter()
            .filter_map(|i| MediaId::parse(&i.cameo_media_id).ok())
            .collect();
        MediaService::new(state.db.clone(), state.cameo.clone())
            .get_many(&user.id, &ids)
            .await
            .map_err(|e| GetListError::Internal(crate::internal!(e)))?
    };

    Ok(Json(items.into_iter().map(MediaItem::from).collect()))
}

/// Add a media item to a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Add item to list")]
pub async fn add_item(
    req: &mut Request,
    depot: &mut Depot,
) -> Result<Json<serde_json::Value>, AddItemError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: AddItemRequest = req.parse_json().await.into_internal()?;

    let svc = ListService::new(state.db);
    let item = svc
        .add_item(&user.id, &id, &body.cameo_media_id, body.notes)
        .await
        .map_err(|e| match e {
            ListError::NotFound | ListError::NotOwner => AddItemError::ListNotFound,
            ListError::Db(ref db_e) if db_e.to_string().contains("duplicate") => {
                AddItemError::AlreadyInList
            }
            e => AddItemError::Internal(crate::internal!(e)),
        })?;

    Ok(Json(
        serde_json::to_value(&item).unwrap_or(serde_json::Value::Null),
    ))
}

/// Remove a media item from a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Remove item from list")]
pub async fn remove_item(req: &mut Request, depot: &mut Depot) -> Result<Json<()>, GetListError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let cameo_id = req.param::<String>("cameo_id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let svc = ListService::new(state.db);
    svc.remove_item(&user.id, &id, &cameo_id)
        .await
        .map_err(|e| match e {
            ListError::NotFound | ListError::NotOwner => GetListError::NotFound,
            e => GetListError::Internal(crate::internal!(e)),
        })?;
    Ok(Json(()))
}

/// Reorder items within a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Reorder list items")]
pub async fn reorder_items(req: &mut Request, depot: &mut Depot) -> Result<Json<()>, ReorderError> {
    let user = require_user(depot)
        .expect("auth middleware populates depot")
        .clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<AppState>().unwrap().clone();
    let body: ReorderRequest = req.parse_json().await.into_internal()?;

    let svc = ListService::new(state.db);
    for item in body.items {
        let reorder = ReorderItem {
            cameo_media_id: item.cameo_media_id,
            before: None,
            after: None,
        };
        svc.reorder_items(&user.id, &id, reorder)
            .await
            .map_err(|e| ReorderError::Internal(crate::internal!(e)))?;
    }
    Ok(Json(()))
}
