use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};

use crate::entity::{api_token, oidc_auth_session, refresh_token};

use super::AuthError;

// --- Refresh tokens ---

pub async fn find_refresh_token(
    db: &DatabaseConnection,
    token_hash: &str,
) -> Result<Option<refresh_token::Model>, AuthError> {
    refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
        .map_err(AuthError::Db)
}

pub async fn delete_refresh_token(db: &DatabaseConnection, id: &str) -> Result<(), AuthError> {
    refresh_token::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map(|_| ())
        .map_err(AuthError::Db)
}

pub async fn delete_user_refresh_tokens(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<(), AuthError> {
    refresh_token::Entity::delete_many()
        .filter(refresh_token::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map(|_| ())
        .map_err(AuthError::Db)
}

// --- OIDC authorization sessions ---

/// Record a pending authorization request so the callback can verify it.
pub async fn insert_oidc_session(
    db: &DatabaseConnection,
    state: &str,
    nonce: &str,
    expires_at: chrono::DateTime<Utc>,
) -> Result<(), AuthError> {
    oidc_auth_session::ActiveModel {
        state: Set(state.to_string()),
        nonce: Set(nonce.to_string()),
        expires_at: Set(expires_at.into()),
        created_at: Set(Utc::now().into()),
    }
    .insert(db)
    .await
    .map(|_| ())
    .map_err(AuthError::Db)
}

/// Consume a pending authorization request. Deleting on read makes `state`
/// single-use, so a replayed callback cannot succeed twice.
pub async fn take_oidc_session(
    db: &DatabaseConnection,
    state: &str,
) -> Result<Option<oidc_auth_session::Model>, AuthError> {
    let found = oidc_auth_session::Entity::find_by_id(state)
        .one(db)
        .await
        .map_err(AuthError::Db)?;
    if found.is_some() {
        oidc_auth_session::Entity::delete_by_id(state)
            .exec(db)
            .await
            .map_err(AuthError::Db)?;
    }
    Ok(found)
}

/// Best-effort cleanup of abandoned authorization requests.
pub async fn purge_expired_oidc_sessions(db: &DatabaseConnection) -> Result<(), AuthError> {
    oidc_auth_session::Entity::delete_many()
        .filter(oidc_auth_session::Column::ExpiresAt.lt(Utc::now()))
        .exec(db)
        .await
        .map(|_| ())
        .map_err(AuthError::Db)
}

// --- API tokens ---

/// Find a token by its deterministic lookup hash — a single indexed query.
pub async fn find_api_token_by_lookup(
    db: &DatabaseConnection,
    lookup: &str,
) -> Result<Option<api_token::Model>, AuthError> {
    api_token::Entity::find()
        .filter(api_token::Column::TokenLookup.eq(lookup))
        .one(db)
        .await
        .map_err(AuthError::Db)
}

/// Tokens issued before m018 have no lookup hash and can only be found by
/// scanning. Restricting the scan to those rows keeps the slow path bounded to
/// pre-existing tokens rather than every token in the table.
pub async fn find_legacy_api_tokens(
    db: &DatabaseConnection,
) -> Result<Vec<api_token::Model>, AuthError> {
    api_token::Entity::find()
        .filter(api_token::Column::TokenLookup.is_null())
        .all(db)
        .await
        .map_err(AuthError::Db)
}

pub async fn insert_api_token(
    db: &DatabaseConnection,
    model: api_token::ActiveModel,
) -> Result<api_token::Model, AuthError> {
    model.insert(db).await.map_err(AuthError::Db)
}

pub async fn delete_api_token(
    db: &DatabaseConnection,
    id: &str,
    user_id: &str,
) -> Result<bool, AuthError> {
    let result = api_token::Entity::delete_many()
        .filter(api_token::Column::Id.eq(id))
        .filter(api_token::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AuthError::Db)?;
    Ok(result.rows_affected > 0)
}

pub async fn list_api_tokens(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<api_token::Model>, AuthError> {
    api_token::Entity::find()
        .filter(api_token::Column::UserId.eq(user_id))
        .filter(
            api_token::Column::ExpiresAt
                .gt(Utc::now())
                .or(api_token::Column::ExpiresAt.is_null()),
        )
        .all(db)
        .await
        .map_err(AuthError::Db)
}
