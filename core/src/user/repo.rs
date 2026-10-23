//! Thin SeaORM queries for users. There is no delete: deactivation is an
//! update, and only [`super::purge`] removes a user.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use crate::entity::user;

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
) -> Result<Option<user::Model>, DbErr> {
    user::Entity::find_by_id(id).one(db).await
}

/// The user signed in as `(issuer, subject)`.
pub async fn find_by_identity<C: ConnectionTrait>(
    db: &C,
    issuer: &str,
    subject: &str,
) -> Result<Option<user::Model>, DbErr> {
    user::Entity::find()
        .filter(user::Column::OidcIssuer.eq(issuer))
        .filter(user::Column::OidcSubject.eq(subject))
        .one(db)
        .await
}

pub async fn list<C: ConnectionTrait>(db: &C) -> Result<Vec<user::Model>, DbErr> {
    user::Entity::find()
        .order_by(user::Column::CreatedAt, Order::Asc)
        .all(db)
        .await
}

pub async fn insert<C: ConnectionTrait>(
    db: &C,
    model: user::ActiveModel,
) -> Result<user::Model, DbErr> {
    model.insert(db).await
}

pub async fn update<C: ConnectionTrait>(
    db: &C,
    model: user::ActiveModel,
) -> Result<user::Model, DbErr> {
    model.update(db).await
}
