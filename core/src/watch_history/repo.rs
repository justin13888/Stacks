use sea_orm::DatabaseConnection;

use crate::entity::watch_history;
use crate::pagination::{CursorParams, PaginatedResult};

use super::WatchHistoryError;

pub async fn insert_event(
    db: &DatabaseConnection,
    model: watch_history::ActiveModel,
) -> Result<watch_history::Model, WatchHistoryError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<watch_history::Model>, WatchHistoryError> {
    let _ = (db, id);
    todo!()
}

pub async fn list_for_user(
    db: &DatabaseConnection,
    user_id: &str,
    cursor: CursorParams,
    cameo_media_id: Option<&str>,
) -> Result<PaginatedResult<watch_history::Model>, WatchHistoryError> {
    let _ = (db, user_id, cursor, cameo_media_id);
    todo!()
}

pub async fn bulk_insert(
    db: &DatabaseConnection,
    models: Vec<watch_history::ActiveModel>,
) -> Result<u64, WatchHistoryError> {
    let _ = (db, models);
    todo!()
}

pub async fn delete_for_user(
    db: &DatabaseConnection,
    user_id: &str,
    id: &str,
) -> Result<bool, WatchHistoryError> {
    let _ = (db, user_id, id);
    todo!()
}
