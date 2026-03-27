use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::entity::watch_history;
use crate::pagination::{CursorParams, PaginatedResult};

use super::WatchHistoryError;

pub async fn insert_event(
    db: &DatabaseConnection,
    model: watch_history::ActiveModel,
) -> Result<watch_history::Model, WatchHistoryError> {
    model.insert(db).await.map_err(WatchHistoryError::Db)
}

pub async fn find_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<watch_history::Model>, WatchHistoryError> {
    watch_history::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(WatchHistoryError::Db)
}

pub async fn list_for_user(
    db: &DatabaseConnection,
    user_id: &str,
    cursor: CursorParams,
    cameo_media_id: Option<&str>,
) -> Result<PaginatedResult<watch_history::Model>, WatchHistoryError> {
    let limit = cursor.limit_or_default();

    let mut query = watch_history::Entity::find()
        .filter(watch_history::Column::UserId.eq(user_id))
        .order_by(watch_history::Column::Timestamp, Order::Desc)
        .order_by(watch_history::Column::Id, Order::Desc);

    if let Some(media_id) = cameo_media_id {
        query = query.filter(watch_history::Column::CameoMediaId.eq(media_id));
    }

    if let Some(ref cursor_id) = cursor.cursor {
        query = query.filter(watch_history::Column::Id.lt(cursor_id.clone()));
    }

    let mut items = query
        .limit(limit + 1)
        .all(db)
        .await
        .map_err(WatchHistoryError::Db)?;

    let next_cursor = if items.len() as u64 > limit {
        let last = items.pop().unwrap();
        Some(last.id.clone())
    } else {
        None
    };

    Ok(PaginatedResult::new(items, next_cursor))
}

pub async fn bulk_insert(
    db: &DatabaseConnection,
    models: Vec<watch_history::ActiveModel>,
) -> Result<u64, WatchHistoryError> {
    if models.is_empty() {
        return Ok(0);
    }
    let result = watch_history::Entity::insert_many(models)
        .exec(db)
        .await
        .map_err(WatchHistoryError::Db)?;
    Ok(result.last_insert_id.len() as u64)
}

pub async fn delete_for_user(
    db: &DatabaseConnection,
    user_id: &str,
    id: &str,
) -> Result<bool, WatchHistoryError> {
    let result = watch_history::Entity::delete_many()
        .filter(watch_history::Column::UserId.eq(user_id))
        .filter(watch_history::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(WatchHistoryError::Db)?;
    Ok(result.rows_affected > 0)
}
