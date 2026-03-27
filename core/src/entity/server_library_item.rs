use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "server_library_items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub cameo_media_id: Option<String>,
    pub server_internal_id: String,
    pub title: String,
    pub quality: Option<String>,
    pub deep_link: String,
    pub playback_link: String,
    pub external_ids: Option<Json>,
    pub last_seen_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::streaming_server::Entity",
        from = "Column::ServerId",
        to = "super::streaming_server::Column::Id",
        on_delete = "Cascade"
    )]
    Server,
}

impl Related<super::streaming_server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Server.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
