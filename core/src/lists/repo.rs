use sea_orm::DatabaseConnection;

use crate::entity::{user_list, user_list_item};

use super::ListError;

pub async fn create_list(
    db: &DatabaseConnection,
    model: user_list::ActiveModel,
) -> Result<user_list::Model, ListError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_list_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<user_list::Model>, ListError> {
    let _ = (db, id);
    todo!()
}

pub async fn find_lists_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<user_list::Model>, ListError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn update_list(
    db: &DatabaseConnection,
    model: user_list::ActiveModel,
) -> Result<user_list::Model, ListError> {
    let _ = (db, model);
    todo!()
}

pub async fn delete_list(db: &DatabaseConnection, id: &str) -> Result<bool, ListError> {
    let _ = (db, id);
    todo!()
}

pub async fn add_item(
    db: &DatabaseConnection,
    model: user_list_item::ActiveModel,
) -> Result<user_list_item::Model, ListError> {
    let _ = (db, model);
    todo!()
}

pub async fn remove_item(
    db: &DatabaseConnection,
    list_id: &str,
    cameo_media_id: &str,
) -> Result<bool, ListError> {
    let _ = (db, list_id, cameo_media_id);
    todo!()
}

pub async fn find_items_for_list(
    db: &DatabaseConnection,
    list_id: &str,
) -> Result<Vec<user_list_item::Model>, ListError> {
    let _ = (db, list_id);
    todo!()
}

pub async fn update_item_position(
    db: &DatabaseConnection,
    list_id: &str,
    cameo_media_id: &str,
    position: f64,
) -> Result<user_list_item::Model, ListError> {
    let _ = (db, list_id, cameo_media_id, position);
    todo!()
}
