//! Tag reads and writes by title only tests use (the `test-support`
//! feature); the API tags entries through [`TagService::assign_tag`].

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::*;

impl TagService {
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
