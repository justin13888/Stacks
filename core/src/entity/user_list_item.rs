use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "user_list_items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub list_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub cameo_media_id: String,
    pub position: f64,
    pub added_at: DateTimeWithTimeZone,
    pub notes: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user_list::Entity",
        from = "Column::ListId",
        to = "super::user_list::Column::Id",
        on_delete = "Cascade"
    )]
    List,
}

impl Related<super::user_list::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::List.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
