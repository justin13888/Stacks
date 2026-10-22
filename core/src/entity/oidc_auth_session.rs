use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A pending OIDC authorization request.
///
/// Created when the client asks for a login URL and consumed (deleted) by the
/// callback. Binding `nonce` to `state` server-side is what makes the callback
/// verifiable: the client never sees — and therefore cannot forge — the nonce,
/// and an unrecognised `state` is rejected outright, which is the CSRF defence.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "oidc_auth_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub state: String,
    pub nonce: String,
    pub expires_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
