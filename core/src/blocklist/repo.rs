use sea_orm::DatabaseConnection;

use crate::entity::blocklist;

pub async fn insert_block(
    db: &DatabaseConnection,
    model: blocklist::ActiveModel,
) -> Result<blocklist::Model, sea_orm::DbErr> {
    let _ = (db, model);
    todo!()
}

pub async fn delete_block(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<bool, sea_orm::DbErr> {
    let _ = (db, user_id, cameo_media_id);
    todo!()
}

pub async fn find_block(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Option<blocklist::Model>, sea_orm::DbErr> {
    let _ = (db, user_id, cameo_media_id);
    todo!()
}

pub async fn list_blocked(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<blocklist::Model>, sea_orm::DbErr> {
    let _ = (db, user_id);
    todo!()
}
