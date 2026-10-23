//! The append-only `entry_events` log.
//!
//! This module only inserts and reads. An event is never changed or removed,
//! and the database refuses both (see the baseline migration).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter,
    QueryOrder, QuerySelect,
};
use serde::Serialize;
use uuid::Uuid;

use crate::entity::entry_event;
use crate::entity::enums::EntryEventKind;
use crate::pagination::{CursorParams, PaginatedResult};

/// Append one event. Call it with the transaction that made the change.
pub async fn record<C, B, A>(
    db: &C,
    entry_id: Uuid,
    user_id: Uuid,
    kind: EntryEventKind,
    before: Option<&B>,
    after: Option<&A>,
) -> Result<entry_event::Model, DbErr>
where
    C: ConnectionTrait,
    B: Serialize,
    A: Serialize,
{
    entry_event::ActiveModel {
        id: Set(Uuid::now_v7()),
        entry_id: Set(entry_id),
        user_id: Set(user_id),
        kind: Set(kind),
        before: Set(before.map(to_json).transpose()?),
        after: Set(after.map(to_json).transpose()?),
        created_at: NotSet,
    }
    .insert(db)
    .await
}

fn to_json<T: Serialize>(v: &T) -> Result<serde_json::Value, DbErr> {
    serde_json::to_value(v).map_err(|e| DbErr::Custom(format!("event payload: {e}")))
}

/// A page of a user's events, newest first, optionally for one entry.
///
/// Keyset-paginated on the UUIDv7 id, which is time-ordered. The cursor is
/// the id of the last item returned; an unparsable cursor starts from the top.
pub async fn list_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    entry_id: Option<Uuid>,
    cursor: &CursorParams,
) -> Result<PaginatedResult<entry_event::Model>, DbErr> {
    let limit = cursor.limit_or_default();
    let mut query = entry_event::Entity::find()
        .filter(entry_event::Column::UserId.eq(user_id))
        .order_by(entry_event::Column::Id, Order::Desc);
    if let Some(entry_id) = entry_id {
        query = query.filter(entry_event::Column::EntryId.eq(entry_id));
    }
    if let Some(after) = cursor
        .cursor
        .as_deref()
        .and_then(|c| Uuid::parse_str(c).ok())
    {
        query = query.filter(entry_event::Column::Id.lt(after));
    }
    let mut items = query.limit(limit + 1).all(db).await?;
    let next_cursor = if items.len() as u64 > limit {
        items.truncate(limit as usize);
        items.last().map(|e| e.id.to_string())
    } else {
        None
    };
    Ok(PaginatedResult::new(items, next_cursor))
}

/// Every event for one entry, oldest first.
pub async fn for_entry<C: ConnectionTrait>(
    db: &C,
    entry_id: Uuid,
) -> Result<Vec<entry_event::Model>, DbErr> {
    entry_event::Entity::find()
        .filter(entry_event::Column::EntryId.eq(entry_id))
        .order_by(entry_event::Column::Id, Order::Asc)
        .all(db)
        .await
}
