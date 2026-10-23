use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::DatePrecision;

/// One watch-through of a title: run 1 is the first watch, later runs are
/// rewatches. Dates carry a precision because imports often know only a year.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "watch_runs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub entry_id: Uuid,
    pub run_number: i32,
    pub started_on: Option<Date>,
    pub started_precision: Option<DatePrecision>,
    pub completed_on: Option<Date>,
    pub completed_precision: Option<DatePrecision>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
