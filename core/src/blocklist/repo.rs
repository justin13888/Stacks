use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::blocklist;

pub async fn insert_block(
    db: &DatabaseConnection,
    model: blocklist::ActiveModel,
) -> Result<blocklist::Model, sea_orm::DbErr> {
    model.insert(db).await
}

pub async fn delete_block(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<bool, sea_orm::DbErr> {
    let result = blocklist::Entity::delete_many()
        .filter(blocklist::Column::UserId.eq(user_id))
        .filter(blocklist::Column::CameoMediaId.eq(cameo_media_id))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

pub async fn find_block(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Option<blocklist::Model>, sea_orm::DbErr> {
    blocklist::Entity::find()
        .filter(blocklist::Column::UserId.eq(user_id))
        .filter(blocklist::Column::CameoMediaId.eq(cameo_media_id))
        .one(db)
        .await
}

pub async fn list_blocked(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<blocklist::Model>, sea_orm::DbErr> {
    blocklist::Entity::find()
        .filter(blocklist::Column::UserId.eq(user_id))
        .all(db)
        .await
}
