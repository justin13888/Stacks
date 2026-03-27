use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder,
};

use crate::entity::{user_list, user_list_item};

use super::ListError;

pub async fn create_list(
    db: &DatabaseConnection,
    model: user_list::ActiveModel,
) -> Result<user_list::Model, ListError> {
    model.insert(db).await.map_err(ListError::Db)
}

pub async fn find_list_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<user_list::Model>, ListError> {
    user_list::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(ListError::Db)
}

pub async fn find_lists_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<user_list::Model>, ListError> {
    user_list::Entity::find()
        .filter(user_list::Column::UserId.eq(user_id))
        .order_by(user_list::Column::CreatedAt, Order::Asc)
        .all(db)
        .await
        .map_err(ListError::Db)
}

pub async fn update_list(
    db: &DatabaseConnection,
    model: user_list::ActiveModel,
) -> Result<user_list::Model, ListError> {
    model.update(db).await.map_err(ListError::Db)
}

pub async fn delete_list(db: &DatabaseConnection, id: &str) -> Result<bool, ListError> {
    let result = user_list::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(ListError::Db)?;
    Ok(result.rows_affected > 0)
}

pub async fn add_item(
    db: &DatabaseConnection,
    model: user_list_item::ActiveModel,
) -> Result<user_list_item::Model, ListError> {
    model.insert(db).await.map_err(ListError::Db)
}

pub async fn remove_item(
    db: &DatabaseConnection,
    list_id: &str,
    cameo_media_id: &str,
) -> Result<bool, ListError> {
    let result = user_list_item::Entity::delete_many()
        .filter(user_list_item::Column::ListId.eq(list_id))
        .filter(user_list_item::Column::CameoMediaId.eq(cameo_media_id))
        .exec(db)
        .await
        .map_err(ListError::Db)?;
    Ok(result.rows_affected > 0)
}

pub async fn find_items_for_list(
    db: &DatabaseConnection,
    list_id: &str,
) -> Result<Vec<user_list_item::Model>, ListError> {
    user_list_item::Entity::find()
        .filter(user_list_item::Column::ListId.eq(list_id))
        .order_by(user_list_item::Column::Position, Order::Asc)
        .all(db)
        .await
        .map_err(ListError::Db)
}

pub async fn count_items_for_list(
    db: &DatabaseConnection,
    list_id: &str,
) -> Result<u64, ListError> {
    user_list_item::Entity::find()
        .filter(user_list_item::Column::ListId.eq(list_id))
        .count(db)
        .await
        .map_err(ListError::Db)
}

pub async fn update_item_position(
    db: &DatabaseConnection,
    list_id: &str,
    cameo_media_id: &str,
    position: f64,
) -> Result<user_list_item::Model, ListError> {
    let item = user_list_item::Entity::find()
        .filter(user_list_item::Column::ListId.eq(list_id))
        .filter(user_list_item::Column::CameoMediaId.eq(cameo_media_id))
        .one(db)
        .await
        .map_err(ListError::Db)?
        .ok_or(ListError::NotFound)?;

    let mut active: user_list_item::ActiveModel = item.into();
    active.position = sea_orm::ActiveValue::Set(position);
    active.update(db).await.map_err(ListError::Db)
}
