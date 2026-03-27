use sea_orm::DatabaseConnection;

use crate::entity::{media_tag, user_tag};

use super::TagError;

pub async fn create_tag(
    db: &DatabaseConnection,
    model: user_tag::ActiveModel,
) -> Result<user_tag::Model, TagError> {
    let _ = (db, model);
    todo!()
}

pub async fn find_tag_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<user_tag::Model>, TagError> {
    let _ = (db, id);
    todo!()
}

pub async fn find_tags_for_user(
    db: &DatabaseConnection,
    user_id: &str,
) -> Result<Vec<user_tag::Model>, TagError> {
    let _ = (db, user_id);
    todo!()
}

pub async fn find_tag_by_name(
    db: &DatabaseConnection,
    user_id: &str,
    name: &str,
) -> Result<Option<user_tag::Model>, TagError> {
    let _ = (db, user_id, name);
    todo!()
}

pub async fn update_tag(
    db: &DatabaseConnection,
    model: user_tag::ActiveModel,
) -> Result<user_tag::Model, TagError> {
    let _ = (db, model);
    todo!()
}

pub async fn delete_tag(db: &DatabaseConnection, id: &str) -> Result<bool, TagError> {
    let _ = (db, id);
    todo!()
}

pub async fn assign_tag(
    db: &DatabaseConnection,
    model: media_tag::ActiveModel,
) -> Result<media_tag::Model, TagError> {
    let _ = (db, model);
    todo!()
}

pub async fn unassign_tag(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
    tag_id: &str,
) -> Result<bool, TagError> {
    let _ = (db, user_id, cameo_media_id, tag_id);
    todo!()
}

pub async fn find_tags_for_media(
    db: &DatabaseConnection,
    user_id: &str,
    cameo_media_id: &str,
) -> Result<Vec<user_tag::Model>, TagError> {
    let _ = (db, user_id, cameo_media_id);
    todo!()
}

pub async fn find_media_for_tag(
    db: &DatabaseConnection,
    user_id: &str,
    tag_id: &str,
) -> Result<Vec<String>, TagError> {
    let _ = (db, user_id, tag_id);
    todo!()
}
