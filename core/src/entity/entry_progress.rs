use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Read-only view: progress derived from runs and non-voided plays.
///
/// Nothing stores these numbers, so nothing can overwrite them; they are
/// recomputed from the append-only plays every time they are read.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "entry_progress")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub entry_id: Uuid,
    pub run_count: i32,
    pub rewatch_count: i32,
    pub play_count: i64,
    pub current_run_plays: i64,
    pub current_run_episodes: i64,
    pub first_watched_at: Option<DateTimeWithTimeZone>,
    pub last_watched_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
