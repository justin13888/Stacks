use sea_orm::DatabaseConnection;

use crate::entity::{notification_channel, notification_preference};

use super::NotificationError;

pub async fn create_channel(
    db: &DatabaseConnection,
    model: notification_channel::ActiveModel,
) -> Result<notification_channel::Model, NotificationError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_channel_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<notification_channel::Model>, NotificationError> {
    let _ = (db, id);
    todo!()
}

pub async fn find_channels_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<notification_channel::Model>, NotificationError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn update_channel(
    db: &DatabaseConnection,
    model: notification_channel::ActiveModel,
) -> Result<notification_channel::Model, NotificationError> {
    let _ = (db, model);
    todo!()
}

pub async fn delete_channel(db: &DatabaseConnection, id: &str) -> Result<bool, NotificationError> {
    let _ = (db, id);
    todo!()
}

pub async fn upsert_preference(
    db: &DatabaseConnection,
    model: notification_preference::ActiveModel,
) -> Result<notification_preference::Model, NotificationError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_preferences_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<notification_preference::Model>, NotificationError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn find_preferences_for_event(
    db: &DatabaseConnection,
    user_id: &str,
    event_type: &str,
) -> Result<Vec<notification_preference::Model>, NotificationError> {
    let _ = (db, user_id, event_type);
    todo!()
}

pub async fn delete_preference(
    db: &DatabaseConnection,
    user_id: &str,
    event_type: &str,
    channel_id: &str,
) -> Result<bool, NotificationError> {
    let _ = (db, user_id, event_type, channel_id);
    todo!()
}
