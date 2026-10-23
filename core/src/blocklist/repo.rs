use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::entity::blocklist;

/// Block a title; blocking it twice is a no-op.
pub async fn insert_block<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    media_id: Uuid,
) -> Result<(), DbErr> {
    use sea_orm::ActiveValue::{NotSet, Set};
    blocklist::Entity::insert(blocklist::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user_id),
        media_id: Set(media_id),
        created_at: NotSet,
        updated_at: NotSet,
    })
    .on_conflict_do_nothing_on([blocklist::Column::UserId, blocklist::Column::MediaId])
    .exec(db)
    .await
    .map(|_| ())
}

pub async fn remove_block<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    media_id: Uuid,
) -> Result<bool, DbErr> {
    let result = blocklist::Entity::delete_many()
        .filter(blocklist::Column::UserId.eq(user_id))
        .filter(blocklist::Column::MediaId.eq(media_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

pub async fn find_block<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    media_id: Uuid,
) -> Result<Option<blocklist::Model>, DbErr> {
    blocklist::Entity::find()
        .filter(blocklist::Column::UserId.eq(user_id))
        .filter(blocklist::Column::MediaId.eq(media_id))
        .one(db)
        .await
}

/// The user's blocked titles, most recent first.
pub async fn list_blocked<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<blocklist::Model>, DbErr> {
    blocklist::Entity::find()
        .filter(blocklist::Column::UserId.eq(user_id))
        .order_by(blocklist::Column::Id, Order::Desc)
        .all(db)
        .await
}
