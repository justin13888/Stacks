use std::collections::{HashMap, HashSet};

use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use uuid::Uuid;

use crate::blocklist::BlocklistService;
use crate::entity::{list, list_item, media};
use crate::media::id::{MediaId, Provider};
use crate::media::models::{Listing, MediaWithUserState};
use crate::media::service as media_service;
use crate::media::{MetadataProvider, catalog, enrichment};
use crate::patch::Patch;

use super::filter::{SmartFilter, matches_filter, parse_filter};
use super::{ListError, fractional_index, repo};

/// Discovery-feed pages scanned per media kind when evaluating a smart list.
/// TMDB pages hold 20 results, so this bounds the candidate pool at 100 items
/// per kind before the filter predicates are applied.
const SMART_LIST_PAGES: u32 = 5;

#[derive(Debug, Clone)]
pub struct CreateListRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_smart: bool,
    pub filter_query: Option<serde_json::Value>,
}

/// A merge patch for a list: absent fields are left untouched, and the
/// nullable ones can be cleared with an explicit `null`.
#[derive(Debug, Clone, Default)]
pub struct UpdateListRequest {
    pub name: Option<String>,
    pub description: Patch<String>,
    /// A smart list's filter. `Null` clears its criteria (the empty filter);
    /// any change on a manual list is [`ListError::NotSmart`].
    pub filter_query: Patch<serde_json::Value>,
}

/// Where an item goes, relative to its neighbours.
///
/// `after` names the item it should follow (`Null`: none, so the start of
/// the list); `before` names the item it should precede (`Null`: none, so
/// the end). Either alone is enough; given both, they must describe the
/// same gap. Neither means the end of the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Placement {
    pub after: Patch<Uuid>,
    pub before: Patch<Uuid>,
}

impl Placement {
    /// Directly after `media_id`.
    pub fn after(media_id: Uuid) -> Self {
        Self {
            after: Patch::Value(media_id),
            before: Patch::Absent,
        }
    }

    /// Directly before `media_id`.
    pub fn before(media_id: Uuid) -> Self {
        Self {
            after: Patch::Absent,
            before: Patch::Value(media_id),
        }
    }

    fn names(&self, media_id: Uuid) -> bool {
        self.after == Patch::Value(media_id) || self.before == Patch::Value(media_id)
    }
}

/// The gap a placement names in `items` (in position order, without the item
/// being placed), as the indices of its lower and upper neighbours.
fn gap(
    items: &[list_item::Model],
    to: &Placement,
) -> Result<(Option<usize>, Option<usize>), ListError> {
    let index = |m: Uuid| {
        items
            .iter()
            .position(|i| i.media_id == m)
            .ok_or(ListError::AnchorNotFound)
    };
    let last = items.len().checked_sub(1);
    let from_after = match to.after {
        Patch::Absent => None,
        Patch::Null => Some((None, (!items.is_empty()).then_some(0))),
        Patch::Value(a) => {
            let i = index(a)?;
            Some((Some(i), (i + 1 < items.len()).then_some(i + 1)))
        }
    };
    let from_before = match to.before {
        Patch::Absent => None,
        Patch::Null => Some((last, None)),
        Patch::Value(b) => {
            let i = index(b)?;
            Some((i.checked_sub(1), Some(i)))
        }
    };
    match (from_after, from_before) {
        (Some(a), Some(b)) if a != b => Err(ListError::InvalidPlacement(
            "`after` and `before` are not neighbours",
        )),
        (Some(g), _) | (None, Some(g)) => Ok(g),
        (None, None) => Ok((last, None)),
    }
}

/// The position for an item placed `to` among `items`.
fn position_for(items: &[list_item::Model], to: &Placement) -> Result<String, ListError> {
    let (lower, upper) = gap(items, to)?;
    let key = |i: Option<usize>| i.and_then(|i| items.get(i)).map(|m| m.position.as_str());
    Ok(fractional_index::between(key(lower), key(upper))?)
}

/// The empty filter: every title the default feed has.
fn empty_filter() -> serde_json::Value {
    serde_json::json!({})
}

pub struct ListService {
    db: DatabaseConnection,
}

impl ListService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Create a list. A smart list always has a filter (the empty one when
    /// none is given); a manual list never does.
    pub async fn create_list(
        &self,
        user_id: Uuid,
        req: CreateListRequest,
    ) -> Result<list::Model, ListError> {
        let name = req.name.trim().to_owned();
        if name.is_empty() {
            return Err(ListError::EmptyName);
        }
        let filter_query = match (req.is_smart, req.filter_query) {
            (false, None) => None,
            (false, Some(_)) => return Err(ListError::NotSmart),
            (true, None) => Some(empty_filter()),
            (true, Some(raw)) => {
                parse_filter(&raw)?;
                Some(raw)
            }
        };
        let model = list::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user_id),
            name: Set(name),
            description: Set(req.description),
            is_smart: Set(req.is_smart),
            filter_query: Set(filter_query),
            deleted_at: Set(None),
            created_at: NotSet,
            updated_at: NotSet,
        };
        Ok(repo::insert_list(&self.db, model).await?)
    }

    /// A live list owned by `user_id`.
    pub async fn get_list(&self, user_id: Uuid, list_id: Uuid) -> Result<list::Model, ListError> {
        owned_live(repo::find_list(&self.db, list_id).await?, user_id)
    }

    pub async fn get_user_lists(&self, user_id: Uuid) -> Result<Vec<list::Model>, ListError> {
        Ok(repo::lists_for_user(&self.db, user_id).await?)
    }

    /// Soft-deleted lists, for the trash view.
    pub async fn trash(&self, user_id: Uuid) -> Result<Vec<list::Model>, ListError> {
        Ok(repo::trashed_lists(&self.db, user_id).await?)
    }

    pub async fn item_count(&self, list_id: Uuid) -> Result<u64, ListError> {
        Ok(repo::count_items(&self.db, list_id).await?)
    }

    /// How many items each list holds; a list with none is absent.
    pub async fn item_counts(&self, list_ids: &[Uuid]) -> Result<HashMap<Uuid, u64>, ListError> {
        Ok(repo::count_items_many(&self.db, list_ids).await?)
    }

    /// Merge-patch a list. Only a smart list has a filter to change, and
    /// clearing it leaves the empty filter.
    pub async fn update_list(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        req: UpdateListRequest,
    ) -> Result<list::Model, ListError> {
        let list = self.get_list(user_id, list_id).await?;
        let name = req.name.map(|n| n.trim().to_owned());
        if name.as_deref().is_some_and(str::is_empty) {
            return Err(ListError::EmptyName);
        }
        let filter_query = match req.filter_query {
            Patch::Absent => Patch::Absent,
            _ if !list.is_smart => return Err(ListError::NotSmart),
            Patch::Null => Patch::Value(empty_filter()),
            Patch::Value(raw) => {
                parse_filter(&raw)?;
                Patch::Value(raw)
            }
        };
        let changed = name.is_some() || !req.description.is_absent() || !filter_query.is_absent();
        if !changed {
            return Ok(list);
        }
        let am = list::ActiveModel {
            id: Unchanged(list.id),
            name: name.map_or(NotSet, Set),
            description: req.description.into_active_value(),
            filter_query: filter_query.into_active_value(),
            ..Default::default()
        };
        Ok(repo::update_list(&self.db, am).await?)
    }

    /// Move a list to the trash. Its items stay and come back on restore.
    pub async fn delete_list(&self, user_id: Uuid, list_id: Uuid) -> Result<(), ListError> {
        self.get_list(user_id, list_id).await?;
        repo::set_list_deleted(&self.db, list_id, true).await?;
        Ok(())
    }

    /// Bring a list back from the trash. Restoring a live list changes
    /// nothing.
    pub async fn restore_list(
        &self,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<list::Model, ListError> {
        let list = repo::find_list(&self.db, list_id)
            .await?
            .filter(|l| l.user_id == user_id)
            .ok_or(ListError::NotFound)?;
        if list.deleted_at.is_none() {
            return Ok(list);
        }
        Ok(repo::set_list_deleted(&self.db, list.id, false).await?)
    }

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

    /// Add a title to a manual list, placed `to`.
    pub async fn add_item_at(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        media_id: Uuid,
        notes: Option<String>,
        to: Placement,
    ) -> Result<list_item::Model, ListError> {
        let txn = self.db.begin().await?;
        let list = lock_list(&txn, user_id, list_id).await?;
        if list.is_smart {
            return Err(ListError::NotManual);
        }
        let items = repo::items_for_list(&txn, list_id).await?;
        if items.iter().any(|i| i.media_id == media_id) {
            return Err(ListError::AlreadyInList);
        }
        let position = position_for(&items, &to)?;
        let item = repo::insert_item(
            &txn,
            list_item::ActiveModel {
                id: Set(Uuid::now_v7()),
                list_id: Set(list_id),
                media_id: Set(media_id),
                position: Set(position),
                notes: Set(notes),
                created_at: NotSet,
                updated_at: NotSet,
            },
        )
        .await
        .map_err(|e| {
            if crate::db::is_unique_violation(&e) {
                ListError::AlreadyInList
            } else {
                ListError::Db(e)
            }
        })?;
        txn.commit().await?;
        Ok(item)
    }

    pub async fn remove_item(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        media_id: Uuid,
    ) -> Result<(), ListError> {
        self.get_list(user_id, list_id).await?;
        if repo::remove_item(&self.db, list_id, media_id).await? {
            Ok(())
        } else {
            Err(ListError::ItemNotFound)
        }
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

    /// Change one item: move it (when `to` is given) and merge-patch its
    /// notes, in one transaction.
    pub async fn update_item(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        media_id: Uuid,
        to: Option<Placement>,
        notes: Patch<String>,
    ) -> Result<list_item::Model, ListError> {
        let txn = self.db.begin().await?;
        lock_list(&txn, user_id, list_id).await?;
        let mut items = repo::items_for_list(&txn, list_id).await?;
        let idx = items
            .iter()
            .position(|i| i.media_id == media_id)
            .ok_or(ListError::ItemNotFound)?;
        let current = items.remove(idx);
        let position = match &to {
            None => NotSet,
            Some(to) if to.names(media_id) => {
                return Err(ListError::InvalidPlacement(
                    "an item cannot be placed next to itself",
                ));
            }
            Some(to) => Set(position_for(&items, to)?),
        };
        if !position.is_set() && notes.is_absent() {
            return Ok(current);
        }
        let item = repo::update_item(
            &txn,
            list_item::ActiveModel {
                id: Unchanged(current.id),
                position,
                notes: notes.into_active_value(),
                ..Default::default()
            },
        )
        .await?;
        txn.commit().await?;
        Ok(item)
    }

    /// A list's stored items in position order, each with its catalog row.
    pub async fn items(
        &self,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<Vec<(list_item::Model, media::Model)>, ListError> {
        self.get_list(user_id, list_id).await?;
        let items = repo::items_for_list(&self.db, list_id).await?;
        let media_ids: Vec<Uuid> = items.iter().map(|i| i.media_id).collect();
        let mut rows = media_rows(&self.db, media_ids).await?;
        // `list_items.media_id` references `media`, so every row is there.
        Ok(items
            .into_iter()
            .filter_map(|i| {
                let m = rows.remove(&i.media_id)?;
                Some((i, m))
            })
            .collect())
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
        let refs = catalog::primary_refs(&self.db, &media_ids).await?;
        let base = items
            .iter()
            .filter_map(|(i, m)| Some(MediaWithUserState::from_snapshot(m, refs.get(&i.media_id)?)))
            .collect();
        Ok(enrichment::enrich(&self.db, user_id, base).await?)
    }

    /// Evaluate a smart list's filter and return the matching titles, once
    /// each, overlaid with the requesting user's library state. Titles the
    /// user blocked are left out.
    ///
    /// Candidates are drawn from a TMDB discovery feed (selected by the
    /// filter's `sort` field) and then narrowed by the filter predicates. The
    /// metadata provider is passed per-call so [`ListService::new`] stays
    /// database-only.
    ///
    /// A feed page that fails ends that kind's scan and is reported in the
    /// listing's `degraded`; the list is an error only when nothing at all
    /// could be fetched.
    pub async fn evaluate_smart_list(
        &self,
        metadata: &dyn MetadataProvider,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<Listing<MediaWithUserState>, ListError> {
        let list = self.get_list(user_id, list_id).await?;
        if !list.is_smart {
            return Err(ListError::NotSmart);
        }
        let filter = match &list.filter_query {
            Some(raw) => parse_filter(raw)?,
            None => SmartFilter::default(),
        };

        // Gather candidates from the chosen feed, one kind at a time, stopping
        // early once a feed runs out of pages.
        let kinds = &filter.kinds;
        let mut results = Vec::new();
        for kind in kinds {
            for page in 1..=SMART_LIST_PAGES {
                let result = metadata
                    .feed(Provider::Tmdb, filter.feed, *kind, page)
                    .await;
                let stop = !matches!(&result, Ok(p) if p.has_next);
                results.push(result);
                if stop {
                    break;
                }
            }
        }

        let listing = media_service::merge_pages(1, results)?;
        let blocked = BlocklistService::new(self.db.clone())
            .blocked_refs(user_id)
            .await?;
        let mut seen = HashSet::new();
        let candidates = listing
            .items
            .into_iter()
            .filter(|m| kinds.contains(&m.kind))
            .filter(|m| MediaId::parse(&m.cameo_media_id).map_or(true, |id| !blocked.contains(&id)))
            .filter(|m| seen.insert(m.cameo_media_id.clone()))
            .collect();
        let enriched = enrichment::enrich(&self.db, user_id, candidates).await?;
        Ok(Listing {
            items: enriched
                .into_iter()
                .filter(|media| matches_filter(media, &filter))
                .collect(),
            next_page: None,
            ..listing
        })
    }
}

/// `list`, when it is live and `user_id`'s.
fn owned_live(list: Option<list::Model>, user_id: Uuid) -> Result<list::Model, ListError> {
    let list = list
        .filter(|l| l.deleted_at.is_none())
        .ok_or(ListError::NotFound)?;
    if list.user_id != user_id {
        return Err(ListError::NotOwner);
    }
    Ok(list)
}

/// The live list `list_id` of `user_id`, locked for the caller's
/// transaction.
async fn lock_list<C: ConnectionTrait>(
    txn: &C,
    user_id: Uuid,
    list_id: Uuid,
) -> Result<list::Model, ListError> {
    owned_live(repo::lock_list(txn, list_id).await?, user_id)
}

async fn media_rows<C: ConnectionTrait>(
    db: &C,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, media::Model>, sea_orm::DbErr> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(media::Entity::find()
        .filter(media::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|m| (m.id, m))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(media: u128, position: &str) -> list_item::Model {
        let now = chrono::Utc::now().into();
        list_item::Model {
            id: Uuid::from_u128(media + 1000),
            list_id: Uuid::nil(),
            media_id: Uuid::from_u128(media),
            position: position.to_owned(),
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn m(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn a_placement_names_one_gap() {
        let items = vec![item(1, "A"), item(2, "B"), item(3, "C")];
        let at = |p: Placement| gap(&items, &p).ok();
        assert_eq!(at(Placement::default()), Some((Some(2), None)));
        assert_eq!(at(Placement::after(m(1))), Some((Some(0), Some(1))));
        assert_eq!(at(Placement::before(m(1))), Some((None, Some(0))));
        assert_eq!(at(Placement::after(m(3))), Some((Some(2), None)));
        let start = Placement {
            after: Patch::Null,
            before: Patch::Absent,
        };
        assert_eq!(at(start), Some((None, Some(0))));
        let end = Placement {
            after: Patch::Absent,
            before: Patch::Null,
        };
        assert_eq!(at(end), Some((Some(2), None)));
        let both = Placement {
            after: Patch::Value(m(1)),
            before: Patch::Value(m(2)),
        };
        assert_eq!(at(both), Some((Some(0), Some(1))));
    }

    #[test]
    fn an_inconsistent_placement_is_refused() {
        let items = vec![item(1, "A"), item(2, "B"), item(3, "C")];
        let apart = Placement {
            after: Patch::Value(m(1)),
            before: Patch::Value(m(3)),
        };
        assert!(matches!(
            gap(&items, &apart),
            Err(ListError::InvalidPlacement(_))
        ));
        assert!(matches!(
            gap(&items, &Placement::after(m(9))),
            Err(ListError::AnchorNotFound)
        ));
        // The start and the end at once, of a non-empty list.
        let nowhere = Placement {
            after: Patch::Null,
            before: Patch::Null,
        };
        assert!(gap(&items, &nowhere).is_err());
        assert_eq!(gap(&[], &nowhere).ok(), Some((None, None)));
    }
}
