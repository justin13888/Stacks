//! Thin SeaORM queries for lists and list items. Lists leave by setting
//! `deleted_at`; only an item's membership row is ever removed.

use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, PaginatorTrait,
    QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::{list, list_item};

pub async fn insert_list<C: ConnectionTrait>(
    db: &C,
    model: list::ActiveModel,
) -> Result<list::Model, DbErr> {
    model.insert(db).await
}

pub async fn find_list<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<Option<list::Model>, DbErr> {
    list::Entity::find_by_id(id).one(db).await
}

/// A user's live lists, oldest first.
pub async fn lists_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<list::Model>, DbErr> {
    list::Entity::find()
        .filter(list::Column::UserId.eq(user_id))
        .filter(list::Column::DeletedAt.is_null())
        .order_by(list::Column::CreatedAt, Order::Asc)
        .all(db)
        .await
}

/// A user's soft-deleted lists, most recently deleted first.
pub async fn trashed_lists<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<list::Model>, DbErr> {
    list::Entity::find()
        .filter(list::Column::UserId.eq(user_id))
        .filter(list::Column::DeletedAt.is_not_null())
        .order_by(list::Column::DeletedAt, Order::Desc)
        .all(db)
        .await
}

pub async fn update_list<C: ConnectionTrait>(
    db: &C,
    model: list::ActiveModel,
) -> Result<list::Model, DbErr> {
    model.update(db).await
}

pub async fn set_list_deleted<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    deleted: bool,
) -> Result<list::Model, DbErr> {
    list::ActiveModel {
        id: Unchanged(id),
        deleted_at: Set(deleted.then(now)),
        ..Default::default()
    }
    .update(db)
    .await
}

pub async fn insert_item<C: ConnectionTrait>(
    db: &C,
    model: list_item::ActiveModel,
) -> Result<list_item::Model, DbErr> {
    model.insert(db).await
}

pub async fn remove_item<C: ConnectionTrait>(
    db: &C,
    list_id: Uuid,
    media_id: Uuid,
) -> Result<bool, DbErr> {
    let result = list_item::Entity::delete_many()
        .filter(list_item::Column::ListId.eq(list_id))
        .filter(list_item::Column::MediaId.eq(media_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

/// A list's items in position order.
pub async fn items_for_list<C: ConnectionTrait>(
    db: &C,
    list_id: Uuid,
) -> Result<Vec<list_item::Model>, DbErr> {
    list_item::Entity::find()
        .filter(list_item::Column::ListId.eq(list_id))
        .order_by(list_item::Column::Position, Order::Asc)
        .all(db)
        .await
}

pub async fn count_items<C: ConnectionTrait>(db: &C, list_id: Uuid) -> Result<u64, DbErr> {
    list_item::Entity::find()
        .filter(list_item::Column::ListId.eq(list_id))
        .count(db)
        .await
}

pub async fn set_item_position<C: ConnectionTrait>(
    db: &C,
    item_id: Uuid,
    position: String,
) -> Result<list_item::Model, DbErr> {
    list_item::ActiveModel {
        id: Unchanged(item_id),
        position: Set(position),
        ..Default::default()
    }
    .update(db)
    .await
}
