use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A title on a list. `position` is a string fractional index
/// (see [`crate::lists::fractional_index`]), ordered bytewise.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "list_items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub list_id: Uuid,
    pub media_id: Uuid,
    pub position: String,
    pub notes: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
