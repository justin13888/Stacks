use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::{api_token, refresh_token};

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

// --- API tokens ---

pub async fn find_api_token_by_prefix(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<api_token::Model>, AuthError> {
    api_token::Entity::find()
        .filter(api_token::Column::UserId.eq(user_id))
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
