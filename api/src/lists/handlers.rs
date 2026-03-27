use salvo::prelude::*;

use super::dto::*;
use super::error::*;

/// List all lists owned by the authenticated user.
#[salvo::oapi::endpoint(tags("Lists"), summary = "List user lists")]
pub async fn list_lists(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<Vec<ListResponse>>, GetListError> {
    todo!()
}

/// Create a new list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Create list")]
pub async fn create_list(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ListResponse>, CreateListError> {
    todo!()
}

/// Get a single list by ID.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Get list")]
pub async fn get_list(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ListResponse>, GetListError> {
    todo!()
}

/// Update list metadata.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Update list")]
pub async fn update_list(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<ListResponse>, UpdateListError> {
    todo!()
}

/// Delete a list by ID.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Delete list")]
pub async fn delete_list(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, DeleteListError> {
    todo!()
}

/// Add a media item to a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Add item to list")]
pub async fn add_item(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<serde_json::Value>, AddItemError> {
    todo!()
}

/// Remove a media item from a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Remove item from list")]
pub async fn remove_item(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, GetListError> {
    todo!()
}

/// Reorder items within a list.
#[salvo::oapi::endpoint(tags("Lists"), summary = "Reorder list items")]
pub async fn reorder_items(
    _req: &mut Request,
    _res: &mut Response,
) -> Result<Json<()>, ReorderError> {
    todo!()
}
