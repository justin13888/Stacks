use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::PlaySource;

/// One recorded viewing of a movie or an episode. Append-only: undo sets
/// `voided_at`, and the database refuses any other change or a delete.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "plays")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub run_id: Uuid,
    pub episode_id: Option<Uuid>,
    pub watched_at: Option<DateTimeWithTimeZone>,
    pub source: PlaySource,
    pub import_id: Option<Uuid>,
    pub voided_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
