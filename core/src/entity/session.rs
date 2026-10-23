use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A sign-in session. Only the SHA-256 of the opaque session token is stored.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub last_seen_at: DateTimeWithTimeZone,
    pub expires_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
