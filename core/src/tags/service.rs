use sea_orm::{ActiveValue::Set, DatabaseConnection};
use ulid::Ulid;

use crate::entity::{media_tag, user_tag};

use super::{TagError, repo};

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
        if repo::find_tag_by_name(&self.db, user_id, &name)
            .await?
            .is_some()
        {
            return Err(TagError::NameTaken);
        }

        let model = user_tag::ActiveModel {
            id: Set(Ulid::new().to_string()),
            user_id: Set(user_id.to_string()),
            name: Set(name),
            color: Set(color),
        };
        repo::create_tag(&self.db, model).await
    }

    pub async fn get_tag(&self, user_id: &str, tag_id: &str) -> Result<user_tag::Model, TagError> {
        let tag = repo::find_tag_by_id(&self.db, tag_id)
            .await?
            .ok_or(TagError::NotFound)?;
        if tag.user_id != user_id {
            return Err(TagError::NotFound);
        }
        Ok(tag)
    }

    pub async fn list_tags(&self, user_id: &str) -> Result<Vec<user_tag::Model>, TagError> {
        repo::find_tags_for_user(&self.db, user_id).await
    }

    pub async fn update_tag(
        &self,
        user_id: &str,
        tag_id: &str,
        name: Option<String>,
        color: Option<String>,
    ) -> Result<user_tag::Model, TagError> {
        let tag = self.get_tag(user_id, tag_id).await?;
        let mut active: user_tag::ActiveModel = tag.into();
        if let Some(n) = name {
            active.name = Set(n);
        }
        if let Some(c) = color {
            active.color = Set(Some(c));
        }
        repo::update_tag(&self.db, active).await
    }

    pub async fn delete_tag(&self, user_id: &str, tag_id: &str) -> Result<(), TagError> {
        self.get_tag(user_id, tag_id).await?;
        repo::delete_tag(&self.db, tag_id).await.map(|_| ())
    }

    pub async fn assign_tag(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        tag_id: &str,
    ) -> Result<(), TagError> {
        // Verify the tag belongs to this user.
        self.get_tag(user_id, tag_id).await?;

        let model = media_tag::ActiveModel {
            user_id: Set(user_id.to_string()),
            cameo_media_id: Set(cameo_media_id.to_string()),
            tag_id: Set(tag_id.to_string()),
        };
        // Ignore duplicate key errors gracefully.
        match repo::assign_tag(&self.db, model).await {
            Ok(_) => Ok(()),
            Err(TagError::Db(e)) if e.to_string().contains("duplicate") => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn unassign_tag(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        tag_id: &str,
    ) -> Result<(), TagError> {
        repo::unassign_tag(&self.db, user_id, cameo_media_id, tag_id)
            .await
            .map(|_| ())
    }

    pub async fn set_tags_for_media(
        &self,
        user_id: &str,
        cameo_media_id: &str,
        tag_ids: Vec<String>,
    ) -> Result<Vec<user_tag::Model>, TagError> {
        // Verify all tags belong to this user.
        for tid in &tag_ids {
            self.get_tag(user_id, tid).await?;
        }

        // Replace: delete all current assignments, then insert new ones.
        repo::clear_tags_for_media(&self.db, user_id, cameo_media_id).await?;

        for tid in &tag_ids {
            let model = media_tag::ActiveModel {
                user_id: Set(user_id.to_string()),
                cameo_media_id: Set(cameo_media_id.to_string()),
                tag_id: Set(tid.clone()),
            };
            repo::assign_tag(&self.db, model).await?;
        }

        repo::find_tags_for_media(&self.db, user_id, cameo_media_id).await
    }

    pub async fn get_tags_for_media(
        &self,
        user_id: &str,
        cameo_media_id: &str,
    ) -> Result<Vec<user_tag::Model>, TagError> {
        repo::find_tags_for_media(&self.db, user_id, cameo_media_id).await
    }

    pub async fn get_media_for_tag(
        &self,
        user_id: &str,
        tag_id: &str,
    ) -> Result<Vec<String>, TagError> {
        repo::find_media_for_tag(&self.db, user_id, tag_id).await
    }
}
