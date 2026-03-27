use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub oidc_sub: String,
    pub username: String,
    pub email: Option<String>,
    pub role: String,
    pub settings: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::api_token::Entity")]
    ApiTokens,
    #[sea_orm(has_many = "super::refresh_token::Entity")]
    RefreshTokens,
    #[sea_orm(has_many = "super::watch_status::Entity")]
    WatchStatuses,
    #[sea_orm(has_many = "super::watch_history::Entity")]
    WatchHistory,
    #[sea_orm(has_many = "super::user_list::Entity")]
    Lists,
    #[sea_orm(has_many = "super::user_tag::Entity")]
    Tags,
    #[sea_orm(has_many = "super::blocklist::Entity")]
    Blocklist,
    #[sea_orm(has_many = "super::notification_channel::Entity")]
    NotificationChannels,
}

impl Related<super::api_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ApiTokens.def()
    }
}

impl Related<super::refresh_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RefreshTokens.def()
    }
}

impl Related<super::watch_status::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::WatchStatuses.def()
    }
}

impl Related<super::watch_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::WatchHistory.def()
    }
}

impl Related<super::user_list::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Lists.def()
    }
}

impl Related<super::user_tag::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tags.def()
    }
}

impl Related<super::blocklist::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Blocklist.def()
    }
}

impl Related<super::notification_channel::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::NotificationChannels.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
