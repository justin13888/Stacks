use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::EntryStatus;

/// A title in a user's library. Soft-deleted via `deleted_at`.
///
/// Progress, rewatch count and first/last watched are not columns: they are
/// derived from `watch_runs` and `plays` (see [`super::entry_progress`]).
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "library_entries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub media_id: Uuid,
    pub status: EntryStatus,
    /// 0–100; the user's display format is a presentation concern.
    pub score: Option<i16>,
    pub favourite: bool,
    pub notes: Option<String>,
    pub deleted_at: Option<DateTimeWithTimeZone>,
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
