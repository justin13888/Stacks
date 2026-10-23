use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A scoped bearer token (`stk_…`) for scripts. Only its SHA-256 is stored;
/// revocation sets `revoked_at` rather than deleting the row.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "api_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    /// The first characters of the plaintext, so a user can recognise a token.
    pub token_prefix: String,
    pub lookup_hash: String,
    /// Validated against [`super::enums::Scope`] and the column's CHECK.
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub last_used_at: Option<DateTimeWithTimeZone>,
    pub revoked_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
