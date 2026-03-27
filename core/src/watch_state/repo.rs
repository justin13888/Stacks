use sea_orm::DatabaseConnection;

use crate::entity::{episode_watch_status, watch_status};

use super::WatchStateError;

pub async fn upsert_status(
    db: &DatabaseConnection,
    model: watch_status::ActiveModel,
) -> Result<watch_status::Model, WatchStateError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_status(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Option<watch_status::Model>, WatchStateError> {
    let _ = (db, user_id, cameo_media_id);
    todo!()
}

pub async fn delete_status(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<bool, WatchStateError> {
    let _ = (db, user_id, cameo_media_id);
    todo!()
}

pub async fn upsert_episode_status(
    db: &DatabaseConnection,
    model: episode_watch_status::ActiveModel,
) -> Result<episode_watch_status::Model, WatchStateError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_episode_status(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_episode_id: &str,
) -> Result<Option<episode_watch_status::Model>, WatchStateError> {
    let _ = (db, user_id, cameo_episode_id);
    todo!()
}

pub async fn find_episodes_for_media(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Vec<episode_watch_status::Model>, WatchStateError> {
    let _ = (db, user_id, cameo_media_id);
    todo!()
}
