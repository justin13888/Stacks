use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "streaming_servers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub server_type: String,
    pub base_url: String,
    pub api_key_enc: Option<Vec<u8>>,
    pub user_id: Option<String>,
    pub webhook_secret: Option<String>,
    pub enabled: bool,
    pub last_synced_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::server_library_item::Entity")]
    LibraryItems,
}

impl Related<super::server_library_item::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::LibraryItems.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
