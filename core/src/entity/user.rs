use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::Role;

/// An account, identified by its OIDC `(issuer, subject)` pair.
///
/// Never hard-deleted: deactivation sets `deactivated_at`, and only the
/// explicit admin purge (`crate::user::purge`) removes a row.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub oidc_issuer: String,
    pub oidc_subject: String,
    pub email: Option<String>,
    pub display_name: String,
    pub role: Role,
    /// See [`crate::user::UserSettings`] for the shape.
    pub settings: Json,
    pub deactivated_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Whether the account may sign in and authenticate requests.
    pub fn is_active(&self) -> bool {
        self.deactivated_at.is_none()
    }

    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
}
