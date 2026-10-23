//! Thin SeaORM queries for sessions, pending OIDC logins and API tokens.

use chrono::Utc;
use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, ExprTrait, Order,
    QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::db::now;
use crate::entity::{api_token, oidc_login_state, session};

// --- Sessions ---

pub async fn insert_session<C: ConnectionTrait>(
    db: &C,
    model: session::ActiveModel,
) -> Result<session::Model, DbErr> {
    model.insert(db).await
}

pub async fn find_session_by_hash<C: ConnectionTrait>(
    db: &C,
    token_hash: &str,
) -> Result<Option<session::Model>, DbErr> {
    session::Entity::find()
        .filter(session::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
}

/// End one session (sign-out or expiry). Sessions are credentials, not user
/// data, so ending one removes the row.
pub async fn end_session<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<(), DbErr> {
    session::Entity::delete_by_id(id).exec(db).await.map(|_| ())
}

/// End one of a user's sessions. Returns whether it existed.
pub async fn end_user_session<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    id: Uuid,
) -> Result<bool, DbErr> {
    session::Entity::delete_many()
        .filter(session::Column::Id.eq(id))
        .filter(session::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map(|r| r.rows_affected > 0)
}

/// Record that a session was just used.
pub async fn touch_session<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<(), DbErr> {
    session::ActiveModel {
        id: Unchanged(id),
        last_seen_at: Set(now()),
        ..Default::default()
    }
    .update(db)
    .await
    .map(|_| ())
}

/// A user's unexpired sessions, newest first.
pub async fn list_sessions<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<session::Model>, DbErr> {
    session::Entity::find()
        .filter(session::Column::UserId.eq(user_id))
        .filter(session::Column::ExpiresAt.gt(Utc::now()))
        .order_by(session::Column::Id, Order::Desc)
        .all(db)
        .await
}

pub async fn end_user_sessions<C: ConnectionTrait>(db: &C, user_id: Uuid) -> Result<(), DbErr> {
    session::Entity::delete_many()
        .filter(session::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map(|_| ())
}

pub async fn end_expired_sessions<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    session::Entity::delete_many()
        .filter(session::Column::ExpiresAt.lt(Utc::now()))
        .exec(db)
        .await
        .map(|_| ())
}

// --- Pending OIDC logins ---

pub async fn insert_login_state<C: ConnectionTrait>(
    db: &C,
    model: oidc_login_state::ActiveModel,
) -> Result<(), DbErr> {
    model.insert(db).await.map(|_| ())
}

/// Consume a pending login. Removing it on read makes `state` single-use, so
/// a replayed callback cannot succeed twice.
pub async fn take_login_state<C: ConnectionTrait>(
    db: &C,
    state: &str,
) -> Result<Option<oidc_login_state::Model>, DbErr> {
    let found = oidc_login_state::Entity::find()
        .filter(oidc_login_state::Column::State.eq(state))
        .one(db)
        .await?;
    if let Some(f) = &found {
        let removed = oidc_login_state::Entity::delete_by_id(f.id)
            .exec(db)
            .await?;
        // Lost a race with a concurrent callback for the same state.
        if removed.rows_affected == 0 {
            return Ok(None);
        }
    }
    Ok(found)
}

/// Best-effort cleanup of abandoned logins.
pub async fn purge_expired_login_states<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    oidc_login_state::Entity::delete_many()
        .filter(oidc_login_state::Column::ExpiresAt.lt(Utc::now()))
        .exec(db)
        .await
        .map(|_| ())
}

// --- API tokens ---

pub async fn find_api_token_by_hash<C: ConnectionTrait>(
    db: &C,
    lookup_hash: &str,
) -> Result<Option<api_token::Model>, DbErr> {
    api_token::Entity::find()
        .filter(api_token::Column::LookupHash.eq(lookup_hash))
        .one(db)
        .await
}

pub async fn insert_api_token<C: ConnectionTrait>(
    db: &C,
    model: api_token::ActiveModel,
) -> Result<api_token::Model, DbErr> {
    model.insert(db).await
}

/// Revoke a user's token. Returns whether a live token was revoked.
pub async fn revoke_api_token<C: ConnectionTrait>(
    db: &C,
    id: Uuid,
    user_id: Uuid,
) -> Result<bool, DbErr> {
    let Some(token) = api_token::Entity::find_by_id(id)
        .one(db)
        .await?
        .filter(|t| t.user_id == user_id && t.revoked_at.is_none())
    else {
        return Ok(false);
    };
    api_token::ActiveModel {
        id: Unchanged(token.id),
        revoked_at: Set(Some(now())),
        ..Default::default()
    }
    .update(db)
    .await
    .map(|_| true)
}

pub async fn touch_api_token<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<(), DbErr> {
    api_token::ActiveModel {
        id: Unchanged(id),
        last_used_at: Set(Some(now())),
        ..Default::default()
    }
    .update(db)
    .await
    .map(|_| ())
}

/// A user's live (unrevoked, unexpired) tokens, newest first.
pub async fn list_api_tokens<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
) -> Result<Vec<api_token::Model>, DbErr> {
    api_token::Entity::find()
        .filter(api_token::Column::UserId.eq(user_id))
        .filter(api_token::Column::RevokedAt.is_null())
        .filter(
            api_token::Column::ExpiresAt
                .gt(Utc::now())
                .or(api_token::Column::ExpiresAt.is_null()),
        )
        .order_by(api_token::Column::Id, Order::Desc)
        .all(db)
        .await
}
