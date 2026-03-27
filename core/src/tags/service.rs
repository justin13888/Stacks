use sea_orm::DatabaseConnection;

use crate::entity::user_tag;

use super::TagError;

pub struct TagService {
    pub db: DatabaseConnection,
}

impl TagService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_tag(
        &self,
        user_id: &str,
        name: String,
        color: Option<String>,
    ) -> Result<user_tag::Model, TagError> {
        let _ = (user_id, name, color);
        todo!()
    }

    pub async fn get_tag(&self, user_id: &str, tag_id: &str) -> Result<user_tag::Model, TagError> {
        let _ = (user_id, tag_id);
        todo!()
    }

    pub async fn list_tags(&self, user_id: &str) -> Result<Vec<user_tag::Model>, TagError> {
        let _ = user_id;
        todo!()
    }

    pub async fn update_tag(
        &self,
        user_id: &str,
        tag_id: &str,
        name: Option<String>,
        color: Option<String>,
    ) -> Result<user_tag::Model, TagError> {
        let _ = (user_id, tag_id, name, color);
        todo!()
    }

    pub async fn delete_tag(&self, user_id: &str, tag_id: &str) -> Result<(), TagError> {
        let _ = (user_id, tag_id);
        todo!()
    }

    pub async fn assign_tag(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        tag_id: &str,
    ) -> Result<(), TagError> {
        let _ = (user_id, cameo_media_id, tag_id);
        todo!()
    }

    pub async fn unassign_tag(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        tag_id: &str,
    ) -> Result<(), TagError> {
        let _ = (user_id, cameo_media_id, tag_id);
        todo!()
    }

    pub async fn get_tags_for_media(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<Vec<user_tag::Model>, TagError> {
        let _ = (user_id, cameo_media_id);
        todo!()
    }

    pub async fn get_media_for_tag(
        &self,
        user_id: &str,
        tag_id: &str,
    ) -> Result<Vec<String>, TagError> {
        let _ = (user_id, tag_id);
        todo!()
    }
}
