use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::ImportOutcome;

/// One row of an import and what became of it. Unmatched rows stay here so
/// they can be resolved by hand later.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "import_items")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub import_id: Uuid,
    pub row_number: i32,
    pub raw: Json,
    pub outcome: ImportOutcome,
    pub matched_media_id: Option<Uuid>,
    pub confidence: Option<f32>,
    pub error: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
