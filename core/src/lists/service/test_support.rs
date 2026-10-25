//! List shorthands only tests use (the `test-support` feature).

use super::*;

impl ListService {
    /// Append a title to the end of a manual list.
    pub async fn add_item(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        media_id: Uuid,
        notes: Option<String>,
    ) -> Result<list_item::Model, ListError> {
        self.add_item_at(user_id, list_id, media_id, notes, Placement::default())
            .await
    }

    /// Move one item relative to its neighbours. Only that item's position
    /// changes.
    pub async fn move_item(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        media_id: Uuid,
        to: Placement,
    ) -> Result<list_item::Model, ListError> {
        self.update_item(user_id, list_id, media_id, Some(to), Patch::Absent)
            .await
    }

    /// A manual list's items as media, in position order, rendered from the
    /// catalog snapshot and overlaid with the user's library state.
    pub async fn list_media(
        &self,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<Vec<MediaWithUserState>, ListError> {
        let items = self.items(user_id, list_id).await?;
        let media_ids: Vec<Uuid> = items.iter().map(|(i, _)| i.media_id).collect();
        let refs = crate::media::catalog::primary_refs(&self.db, &media_ids).await?;
        let base = items
            .iter()
            .filter_map(|(i, m)| Some(MediaWithUserState::from_snapshot(m, refs.get(&i.media_id)?)))
            .collect();
        Ok(enrichment::enrich(&self.db, user_id, base).await?)
    }
}
