//! A smart list's filter on its way into an account.

use sea_orm::EntityTrait;
use serde_json::Value;

use crate::entity::tag;
use crate::media::filter::FilterQuery;

use super::super::format;
use super::{RestoreError, Restorer};

/// A stored filter in its canonical form, so two that mean the same compare
/// equal; one that does not read as a filter is kept as it is.
pub(super) fn canonical_filter(stored: &Option<Value>) -> Option<Value> {
    stored.as_ref().map(|raw| {
        FilterQuery::from_stored(raw)
            .and_then(|f| serde_json::to_value(f).ok())
            .unwrap_or_else(|| raw.clone())
    })
}

impl Restorer<'_> {
    /// A list's filter as this account stores it: each tag it names under
    /// the tag's local id. A tag the backup lacks (deleted before the backup
    /// was taken) keeps its id, and still matches nothing; one that is
    /// another account's tag is refused, as creating the list would be.
    pub(super) async fn local_filter(
        &self,
        l: &format::List,
    ) -> Result<Option<Value>, RestoreError> {
        let Some(filter) = &l.filter else {
            return Ok(None);
        };
        let mut filter = filter.clone();
        for tag in filter.tag.iter_mut().flatten() {
            if let Some(local) = self.ids.get(tag) {
                *tag = *local;
            } else if tag::Entity::find_by_id(*tag)
                .one(self.txn)
                .await?
                .is_some_and(|t| t.user_id != self.user_id)
            {
                return Err(RestoreError::Invalid(format!(
                    "list {}'s filter names a tag that is not yours",
                    l.id
                )));
            }
        }
        serde_json::to_value(filter)
            .map(Some)
            .map_err(|e| RestoreError::Invalid(format!("list {}'s filter: {e}", l.id)))
    }
}
