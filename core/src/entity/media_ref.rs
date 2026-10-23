use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::{MediaKind, RefProvider};

/// One external id for a title. Links the same title across providers; the
/// row with `is_primary` is the public, self-describing id.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "media_refs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub media_id: Uuid,
    pub provider: RefProvider,
    pub kind: MediaKind,
    pub external_id: String,
    pub is_primary: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::media::Entity",
        from = "Column::MediaId",
        to = "super::media::Column::Id",
        on_delete = "Restrict"
    )]
    Media,
}

impl Related<super::media::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Media.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
