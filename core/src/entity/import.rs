use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::{ConflictStrategy, ImportSource, ImportStatus};

/// One uploaded import. The original file is kept (up to the configured size
/// cap) so an import can be re-examined or re-run later.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "imports")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub source: ImportSource,
    pub status: ImportStatus,
    pub conflict_strategy: ConflictStrategy,
    pub original_filename: Option<String>,
    #[serde(skip)]
    pub original_file: Option<Vec<u8>>,
    pub original_size: i64,
    pub total_count: i32,
    pub applied_count: i32,
    pub skipped_count: i32,
    pub unmatched_count: i32,
    pub failed_count: i32,
    pub error: Option<String>,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub completed_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
