use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, TransactionTrait};
use uuid::Uuid;

use crate::entity::{library_entry, tag};
use crate::library::repo as library_repo;
use crate::patch::Patch;

use super::{TagError, repo};

/// Tags belong to a user and are assigned to that user's library entries.
pub struct TagService {
    db: DatabaseConnection,
}

impl TagService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_tag(
        &self,
        user_id: Uuid,
        name: String,
        color: Option<String>,
    ) -> Result<tag::Model, TagError> {
        if name.trim().is_empty() {
            return Err(TagError::EmptyName);
        }
        repo::insert_tag(
            &self.db,
            tag::ActiveModel {
                id: Set(Uuid::now_v7()),
                user_id: Set(user_id),
                name: Set(name),
                color: Set(color),
                created_at: NotSet,
                updated_at: NotSet,
            },
        )
        .await
        .map_err(name_taken)
    }

    /// A tag owned by `user_id`.
    pub async fn get_tag(&self, user_id: Uuid, tag_id: Uuid) -> Result<tag::Model, TagError> {
        repo::find_tag(&self.db, tag_id)
            .await?
            .filter(|t| t.user_id == user_id)
            .ok_or(TagError::NotFound)
    }

    pub async fn list_tags(&self, user_id: Uuid) -> Result<Vec<tag::Model>, TagError> {
        Ok(repo::tags_for_user(&self.db, user_id).await?)
    }

    /// Merge-patch a tag: absent fields are untouched, `color: null` clears.
    pub async fn update_tag(
        &self,
        user_id: Uuid,
        tag_id: Uuid,
        name: Option<String>,
        color: Patch<String>,
    ) -> Result<tag::Model, TagError> {
        let tag = self.get_tag(user_id, tag_id).await?;
        if name.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return Err(TagError::EmptyName);
        }
        if name.is_none() && color.is_absent() {
            return Ok(tag);
        }
        let am = tag::ActiveModel {
            id: Unchanged(tag.id),
            name: name.map_or(NotSet, Set),
            color: color.into_active_value(),
            ..Default::default()
        };
        repo::update_tag(&self.db, am).await.map_err(name_taken)
    }

    /// Delete a tag and unassign it everywhere, atomically. A tag is a label,
    /// not library data: the entries it was on are untouched.
    pub async fn delete_tag(&self, user_id: Uuid, tag_id: Uuid) -> Result<(), TagError> {
        self.get_tag(user_id, tag_id).await?;
        let txn = self.db.begin().await?;
        repo::remove_tag(&txn, tag_id).await?;
        txn.commit().await?;
        Ok(())
    }

    async fn live_entry(
        &self,
        user_id: Uuid,
        media_id: Uuid,
    ) -> Result<library_entry::Model, TagError> {
        library_repo::find_entry(&self.db, user_id, media_id)
            .await?
            .filter(|e| e.deleted_at.is_none())
            .ok_or(TagError::NotTracked)
    }

    pub async fn assign_tag(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        tag_id: Uuid,
    ) -> Result<(), TagError> {
        self.get_tag(user_id, tag_id).await?;
        let entry = self.live_entry(user_id, media_id).await?;
        Ok(repo::assign(&self.db, entry.id, tag_id).await?)
    }

    pub async fn unassign_tag(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        tag_id: Uuid,
    ) -> Result<(), TagError> {
        let entry = self.live_entry(user_id, media_id).await?;
        repo::unassign(&self.db, entry.id, tag_id).await?;
        Ok(())
    }

    /// Replace a title's tags with exactly `tag_ids`, atomically.
    pub async fn set_tags_for_media(
        &self,
        user_id: Uuid,
        media_id: Uuid,
        tag_ids: Vec<Uuid>,
    ) -> Result<Vec<tag::Model>, TagError> {
        for tid in &tag_ids {
            self.get_tag(user_id, *tid).await?;
        }
        let entry = self.live_entry(user_id, media_id).await?;
        let txn = self.db.begin().await?;
        repo::clear_entry(&txn, entry.id).await?;
        for tid in &tag_ids {
            repo::assign(&txn, entry.id, *tid).await?;
        }
        let tags = repo::tags_for_entry(&txn, entry.id).await?;
        txn.commit().await?;
        Ok(tags)
    }

    /// A title's tags; none when it is not in the library.
    pub async fn get_tags_for_media(
        &self,
        user_id: Uuid,
        media_id: Uuid,
    ) -> Result<Vec<tag::Model>, TagError> {
        match library_repo::find_entry(&self.db, user_id, media_id).await? {
            Some(e) => Ok(repo::tags_for_entry(&self.db, e.id).await?),
            None => Ok(Vec::new()),
        }
    }

    /// The catalog ids of the live entries carrying a tag.
    pub async fn get_media_for_tag(
        &self,
        user_id: Uuid,
        tag_id: Uuid,
    ) -> Result<Vec<Uuid>, TagError> {
        self.get_tag(user_id, tag_id).await?;
        let entry_ids = repo::entries_for_tag(&self.db, tag_id).await?;
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(library_entry::Entity::find()
            .filter(library_entry::Column::Id.is_in(entry_ids))
            .filter(library_entry::Column::DeletedAt.is_null())
            .all(&self.db)
            .await?
            .into_iter()
            .map(|e| e.media_id)
            .collect())
    }
}

fn name_taken(e: sea_orm::DbErr) -> TagError {
    if crate::db::is_unique_violation(&e) {
        TagError::NameTaken
    } else {
        TagError::Db(e)
    }
}
