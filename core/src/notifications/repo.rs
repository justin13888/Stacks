use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    sea_query::OnConflict,
};

use crate::entity::{notification_channel, notification_preference};

use super::NotificationError;

pub async fn create_channel(
    db: &DatabaseConnection,
    model: notification_channel::ActiveModel,
) -> Result<notification_channel::Model, NotificationError> {
    model.insert(db).await.map_err(NotificationError::Db)
}

pub async fn find_channel_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<notification_channel::Model>, NotificationError> {
    notification_channel::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(NotificationError::Db)
}

pub async fn find_channels_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<notification_channel::Model>, NotificationError> {
    notification_channel::Entity::find()
        .filter(notification_channel::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(NotificationError::Db)
}

pub async fn update_channel(
    db: &DatabaseConnection,
    model: notification_channel::ActiveModel,
) -> Result<notification_channel::Model, NotificationError> {
    model.update(db).await.map_err(NotificationError::Db)
}

pub async fn delete_channel(db: &DatabaseConnection, id: &str) -> Result<bool, NotificationError> {
    let result = notification_channel::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(NotificationError::Db)?;
    Ok(result.rows_affected > 0)
}

pub async fn upsert_preference(
    db: &DatabaseConnection,
    model: notification_preference::ActiveModel,
) -> Result<notification_preference::Model, NotificationError> {
    notification_preference::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([
                notification_preference::Column::UserId,
                notification_preference::Column::EventType,
                notification_preference::Column::ChannelId,
            ])
            .update_columns([
                notification_preference::Column::Enabled,
                notification_preference::Column::Config,
            ])
            .to_owned(),
        )
        .exec_with_returning(db)
        .await
        .map_err(NotificationError::Db)
}

pub async fn find_preferences_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<notification_preference::Model>, NotificationError> {
    notification_preference::Entity::find()
        .filter(notification_preference::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(NotificationError::Db)
}

pub async fn find_preferences_for_event(
    db: &DatabaseConnection,
    user_id: &str,
    event_type: &str,
) -> Result<Vec<notification_preference::Model>, NotificationError> {
    notification_preference::Entity::find()
        .filter(notification_preference::Column::UserId.eq(user_id))
        .filter(notification_preference::Column::EventType.eq(event_type))
        .all(db)
        .await
        .map_err(NotificationError::Db)
}

pub async fn delete_preference(
    db: &DatabaseConnection,
    user_id: &str,
    event_type: &str,
    channel_id: &str,
) -> Result<bool, NotificationError> {
    let result = notification_preference::Entity::delete_many()
        .filter(notification_preference::Column::UserId.eq(user_id))
        .filter(notification_preference::Column::EventType.eq(event_type))
        .filter(notification_preference::Column::ChannelId.eq(channel_id))
        .exec(db)
        .await
        .map_err(NotificationError::Db)?;
    Ok(result.rows_affected > 0)
}
