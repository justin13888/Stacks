use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::user;

use super::UserError;

pub async fn find_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<user::Model>, sea_orm::DbErr> {
    user::Entity::find_by_id(id).one(db).await
}

pub async fn find_by_oidc_sub(
    db: &DatabaseConnection,
    sub: &str,
) -> Result<Option<user::Model>, sea_orm::DbErr> {
    user::Entity::find()
        .filter(user::Column::OidcSub.eq(sub))
        .one(db)
        .await
}

pub async fn list(db: &DatabaseConnection) -> Result<Vec<user::Model>, UserError> {
    user::Entity::find().all(db).await.map_err(UserError::Db)
}

pub async fn insert(
    db: &DatabaseConnection,
    model: user::ActiveModel,
) -> Result<user::Model, sea_orm::DbErr> {
    model.insert(db).await
}

pub async fn update(
    db: &DatabaseConnection,
    model: user::ActiveModel,
) -> Result<user::Model, UserError> {
    model.update(db).await.map_err(UserError::Db)
}

pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<bool, UserError> {
    let result = user::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(UserError::Db)?;
    Ok(result.rows_affected > 0)
}
