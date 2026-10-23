use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::enums::NotificationEventType;
use crate::entity::{notification_channel, notification_subscription};

pub async fn create_channel<C: ConnectionTrait>(
    db: &C,
    model: notification_channel::ActiveModel,
) -> Result<notification_channel::Model, DbErr> {
    model.insert(db).await
}

pub async fn find_channel<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
) -> Result<Option<notification_channel::Model>, DbErr> {
    notification_channel::Entity::find_by_id(id).one(db).await
}

pub async fn channels_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<notification_channel::Model>, DbErr> {
    notification_channel::Entity::find()
        .filter(notification_channel::Column::UserId.eq(user_id))
        .order_by(notification_channel::Column::Id, Order::Asc)
        .all(db)
        .await
}

pub async fn update_channel<C: ConnectionTrait>(
    db: &C,
    model: notification_channel::ActiveModel,
) -> Result<notification_channel::Model, DbErr> {
    model.update(db).await
}

/// Remove a channel and its subscriptions (the caller's transaction makes the
/// pair atomic). Channel configuration is a setting, not library data.
pub async fn remove_channel<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<bool, DbErr> {
    notification_subscription::Entity::delete_many()
        .filter(notification_subscription::Column::ChannelId.eq(id))
        .exec(db)
        .await?;
    let result = notification_channel::Entity::delete_by_id(id)
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

pub async fn upsert_subscription<C: ConnectionTrait>(
    db: &C,
    model: notification_subscription::ActiveModel,
) -> Result<notification_subscription::Model, DbErr> {
    notification_subscription::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                notification_subscription::Column::UserId,
                notification_subscription::Column::EventType,
                notification_subscription::Column::ChannelId,
            ])
            .update_columns([
                notification_subscription::Column::Enabled,
                notification_subscription::Column::Config,
            ])
            .to_owned(),
        )
        .exec_with_returning(db)
        .await
}

pub async fn subscriptions_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<notification_subscription::Model>, DbErr> {
    notification_subscription::Entity::find()
        .filter(notification_subscription::Column::UserId.eq(user_id))
        .all(db)
        .await
}

pub async fn subscriptions_for_event<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    event_type: NotificationEventType,
) -> Result<Vec<notification_subscription::Model>, DbErr> {
    notification_subscription::Entity::find()
        .filter(notification_subscription::Column::UserId.eq(user_id))
        .filter(notification_subscription::Column::EventType.eq(event_type))
        .all(db)
        .await
}

pub async fn remove_subscription<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    event_type: NotificationEventType,
    channel_id: Uuid,
) -> Result<bool, DbErr> {
    let result = notification_subscription::Entity::delete_many()
        .filter(notification_subscription::Column::UserId.eq(user_id))
        .filter(notification_subscription::Column::EventType.eq(event_type))
        .filter(notification_subscription::Column::ChannelId.eq(channel_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}
