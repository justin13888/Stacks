use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::entity::{media_tag, user_tag};

use super::TagError;

pub async fn create_tag(
    db: &DatabaseConnection,
    model: user_tag::ActiveModel,
) -> Result<user_tag::Model, TagError> {
    model.insert(db).await.map_err(TagError::Db)
}

pub async fn find_tag_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<user_tag::Model>, TagError> {
    user_tag::Entity::find_by_id(id)
        .one(db)
        .await
        .map_err(TagError::Db)
}

pub async fn find_tags_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<user_tag::Model>, TagError> {
    user_tag::Entity::find()
        .filter(user_tag::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(TagError::Db)
}

pub async fn find_tag_by_name(
    db: &DatabaseConnection,
    user_id: &str,
    name: &str,
) -> Result<Option<user_tag::Model>, TagError> {
    user_tag::Entity::find()
        .filter(user_tag::Column::UserId.eq(user_id))
        .filter(user_tag::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(TagError::Db)
}

pub async fn update_tag(
    db: &DatabaseConnection,
    model: user_tag::ActiveModel,
) -> Result<user_tag::Model, TagError> {
    model.update(db).await.map_err(TagError::Db)
}

pub async fn delete_tag(db: &DatabaseConnection, id: &str) -> Result<bool, TagError> {
    let result = user_tag::Entity::delete_by_id(id)
        .exec(db)
        .await
        .map_err(TagError::Db)?;
    Ok(result.rows_affected > 0)
}

pub async fn assign_tag(
    db: &DatabaseConnection,
    model: media_tag::ActiveModel,
) -> Result<media_tag::Model, TagError> {
    model.insert(db).await.map_err(TagError::Db)
}

pub async fn unassign_tag(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
    tag_id: &str,
) -> Result<bool, TagError> {
    let result = media_tag::Entity::delete_many()
        .filter(media_tag::Column::UserId.eq(user_id))
        .filter(media_tag::Column::CameoMediaId.eq(cameo_media_id))
        .filter(media_tag::Column::TagId.eq(tag_id))
        .exec(db)
        .await
        .map_err(TagError::Db)?;
    Ok(result.rows_affected > 0)
}

/// Delete all tag assignments for a media item (used by set_media_tags replace).
pub async fn clear_tags_for_media(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<(), TagError> {
    media_tag::Entity::delete_many()
        .filter(media_tag::Column::UserId.eq(user_id))
        .filter(media_tag::Column::CameoMediaId.eq(cameo_media_id))
        .exec(db)
        .await
        .map(|_| ())
        .map_err(TagError::Db)
}

pub async fn find_tags_for_media(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Vec<user_tag::Model>, TagError> {
    // Find all tag_ids for this user+media, then load the user_tag models.
    let tag_ids: Vec<String> = media_tag::Entity::find()
        .filter(media_tag::Column::UserId.eq(user_id))
        .filter(media_tag::Column::CameoMediaId.eq(cameo_media_id))
        .all(db)
        .await
        .map_err(TagError::Db)?
        .into_iter()
        .map(|mt| mt.tag_id)
        .collect();

    if tag_ids.is_empty() {
        return Ok(vec![]);
    }

    user_tag::Entity::find()
        .filter(user_tag::Column::Id.is_in(tag_ids))
        .all(db)
        .await
        .map_err(TagError::Db)
}

pub async fn find_media_for_tag(
    db: &DatabaseConnection,
    user_id: &str,
    tag_id: &str,
) -> Result<Vec<String>, TagError> {
    media_tag::Entity::find()
        .filter(media_tag::Column::UserId.eq(user_id))
        .filter(media_tag::Column::TagId.eq(tag_id))
        .all(db)
        .await
        .map_err(TagError::Db)
        .map(|rows| rows.into_iter().map(|r| r.cameo_media_id).collect())
}
