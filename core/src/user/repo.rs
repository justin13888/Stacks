//! Thin SeaORM queries for users. There is no delete: deactivation is an
//! update, and only [`super::purge`] removes a user.

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder, QuerySelect,
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

/// One page of users, newest first (ids are UUIDv7, so time-ordered), before
/// `before` when given. At most `limit` rows; ask for one more than a page
/// to learn whether another follows.
pub async fn page<C: ConnectionTrait>(
    db: &C,
    before: Option<Uuid>,
    limit: u64,
) -> Result<Vec<user::Model>, DbErr> {
    let mut query = user::Entity::find().order_by(user::Column::Id, Order::Desc);
    if let Some(before) = before {
        query = query.filter(user::Column::Id.lt(before));
    }
    query.limit(limit).all(db).await
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
