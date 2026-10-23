use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A pending OIDC authorization request.
///
/// Created when the client asks for a login URL and consumed by the callback.
/// Binding `nonce` and the PKCE verifier to `state` server-side is what makes
/// the callback verifiable: the client never sees either, and an unrecognised
/// `state` is rejected outright, which is the CSRF defence.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "oidc_login_states")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub redirect_to: Option<String>,
    pub expires_at: DateTimeWithTimeZone,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
