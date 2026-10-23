use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::NotificationEventType;

/// Routes one event type to one of the user's channels.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "notification_subscriptions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub channel_id: Uuid,
    pub event_type: NotificationEventType,
    pub enabled: bool,
    pub config: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
