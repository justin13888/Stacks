use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::EntryEventKind;

/// Append-only audit of a library mutation, written in the same transaction
/// as the mutation it records. Backs the history feed and undo.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "entry_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub entry_id: Uuid,
    pub user_id: Uuid,
    pub kind: EntryEventKind,
    pub before: Option<Json>,
    pub after: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
