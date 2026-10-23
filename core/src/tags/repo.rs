//! Thin SeaORM queries for tags and their assignments to library entries.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::{entry_tag, tag};

pub async fn insert_tag<C: ConnectionTrait>(
    db: &C,
    model: tag::ActiveModel,
) -> Result<tag::Model, DbErr> {
    model.insert(db).await
}

pub async fn find_tag<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<Option<tag::Model>, DbErr> {
    tag::Entity::find_by_id(id).one(db).await
}

pub async fn tags_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<tag::Model>, DbErr> {
    tag::Entity::find()
        .filter(tag::Column::UserId.eq(user_id))
        .order_by(tag::Column::Name, Order::Asc)
        .all(db)
        .await
}

pub async fn update_tag<C: ConnectionTrait>(
    db: &C,
    model: tag::ActiveModel,
) -> Result<tag::Model, DbErr> {
    model.update(db).await
}

/// Remove a tag and every assignment of it (the caller's transaction makes
/// the pair atomic).
pub async fn remove_tag<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<bool, DbErr> {
    entry_tag::Entity::delete_many()
        .filter(entry_tag::Column::TagId.eq(id))
        .exec(db)
        .await?;
    let result = tag::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}

/// Assign a tag to an entry; assigning it twice is a no-op.
pub async fn assign<C: ConnectionTrait>(db: &C, entry_id: Uuid, tag_id: Uuid) -> Result<(), DbErr> {
    use sea_orm::ActiveValue::{NotSet, Set};
    entry_tag::Entity::insert(entry_tag::ActiveModel {
        id: Set(Uuid::now_v7()),
        entry_id: Set(entry_id),
        tag_id: Set(tag_id),
        created_at: NotSet,
        updated_at: NotSet,
    })
    .on_conflict_do_nothing_on([entry_tag::Column::EntryId, entry_tag::Column::TagId])
    .exec(db)
    .await
    .map(|_| ())
}

pub async fn unassign<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
    tag_id: Uuid,
) -> Result<bool, DbErr> {
    let result = entry_tag::Entity::delete_many()
        .filter(entry_tag::Column::EntryId.eq(entry_id))
        .filter(entry_tag::Column::TagId.eq(tag_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

/// Remove every assignment from an entry.
pub async fn clear_entry<C: ConnectionTrait>(db: &C, entry_id: Uuid) -> Result<(), DbErr> {
    entry_tag::Entity::delete_many()
        .filter(entry_tag::Column::EntryId.eq(entry_id))
        .exec(db)
        .await
        .map(|_| ())
}

pub async fn tags_for_entry<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
) -> Result<Vec<tag::Model>, DbErr> {
    let ids: Vec<Uuid> = entry_tag::Entity::find()
        .filter(entry_tag::Column::EntryId.eq(entry_id))
        .all(db)
        .await?
        .into_iter()
        .map(|t| t.tag_id)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    tag::Entity::find()
        .filter(tag::Column::Id.is_in(ids))
        .order_by(tag::Column::Name, Order::Asc)
        .all(db)
        .await
}

pub async fn entries_for_tag<C: ConnectionTrait>(db: &C, tag_id: Uuid) -> Result<Vec<Uuid>, DbErr> {
    Ok(entry_tag::Entity::find()
        .filter(entry_tag::Column::TagId.eq(tag_id))
        .all(db)
        .await?
        .into_iter()
        .map(|t| t.entry_id)
        .collect())
}
