use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use sea_orm::{DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{list, list_item, media};
use crate::media::filter::FilterQuery;
use crate::media::id::{MediaKind, Provider};
use crate::media::models::{Listing, MediaWithUserState};
use crate::media::service as media_service;
use crate::media::{DiscoverCategory, MetadataProvider, catalog, enrichment};
use crate::patch::Patch;

use super::{ListError, fractional_index, repo};

/// Discovery-feed pages scanned per media kind when evaluating a smart list.
/// TMDB pages hold 20 results, so this bounds the candidate pool at 100 items
/// per kind before the filter predicates are applied.
const SMART_LIST_PAGES: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateListRequest {
    pub name: String,
    pub description: Option<String>,
    pub is_smart: bool,
    pub filter_query: Option<serde_json::Value>,
}

/// A merge patch for a list: absent fields are left untouched, and the
/// nullable ones can be cleared with an explicit `null`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateListRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Patch<String>,
    #[serde(default)]
    pub filter_query: Patch<serde_json::Value>,
}

/// Where to move an item: after one item, before another, or (both `None`)
/// to the end.
#[derive(Debug, Clone, Default)]
pub struct MoveItem {
    pub after: Option<Uuid>,
    pub before: Option<Uuid>,
}

pub struct ListService {
    db: DatabaseConnection,
}

impl ListService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_list(
        &self,
        user_id: Uuid,
        req: CreateListRequest,
    ) -> Result<list::Model, ListError> {
        if req.name.trim().is_empty() {
            return Err(ListError::EmptyName);
        }
        let model = list::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user_id),
            name: Set(req.name),
            description: Set(req.description),
            is_smart: Set(req.is_smart),
            filter_query: Set(req.filter_query),
            deleted_at: Set(None),
            created_at: NotSet,
            updated_at: NotSet,
        };
        Ok(repo::insert_list(&self.db, model).await?)
    }

    /// A live list owned by `user_id`.
    pub async fn get_list(&self, user_id: Uuid, list_id: Uuid) -> Result<list::Model, ListError> {
        let list = repo::find_list(&self.db, list_id)
            .await?
            .filter(|l| l.deleted_at.is_none())
            .ok_or(ListError::NotFound)?;
        if list.user_id != user_id {
            return Err(ListError::NotOwner);
        }
        Ok(list)
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

    pub async fn update_list(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        req: UpdateListRequest,
    ) -> Result<list::Model, ListError> {
        let list = self.get_list(user_id, list_id).await?;
        if req.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return Err(ListError::EmptyName);
        }
        let changed =
            req.name.is_some() || !req.description.is_absent() || !req.filter_query.is_absent();
        if !changed {
            return Ok(list);
        }
        let am = list::ActiveModel {
            id: Unchanged(list.id),
            name: req.name.map_or(NotSet, Set),
            description: req.description.into_active_value(),
            filter_query: req.filter_query.into_active_value(),
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

    pub async fn restore_list(
        &self,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<list::Model, ListError> {
        let list = repo::find_list(&self.db, list_id)
            .await?
            .filter(|l| l.user_id == user_id && l.deleted_at.is_some())
            .ok_or(ListError::NotFound)?;
        Ok(repo::set_list_deleted(&self.db, list.id, false).await?)
    }

    /// Append a title to the end of a list.
    pub async fn add_item(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        media_id: Uuid,
        notes: Option<String>,
    ) -> Result<list_item::Model, ListError> {
        self.get_list(user_id, list_id).await?;
        let txn = self.db.begin().await?;
        let items = repo::items_for_list(&txn, list_id).await?;
        if items.iter().any(|i| i.media_id == media_id) {
            return Err(ListError::AlreadyInList);
        }
        let position = fractional_index::between(items.last().map(|i| i.position.as_str()), None)?;
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
            Err(ListError::NotFound)
        }
    }

    /// Move one item relative to its neighbours. Only that item's position
    /// changes.
    pub async fn move_item(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        media_id: Uuid,
        to: MoveItem,
    ) -> Result<list_item::Model, ListError> {
        self.get_list(user_id, list_id).await?;
        let txn = self.db.begin().await?;
        let mut items = repo::items_for_list(&txn, list_id).await?;
        let idx = items
            .iter()
            .position(|i| i.media_id == media_id)
            .ok_or(ListError::NotFound)?;
        let moving = items.remove(idx);
        let find = |m: Uuid| {
            items
                .iter()
                .position(|i| i.media_id == m)
                .ok_or(ListError::NotFound)
        };
        let (lower, upper) = match (to.after, to.before) {
            (Some(after), _) => {
                let i = find(after)?;
                (Some(i), (i + 1 < items.len()).then_some(i + 1))
            }
            (None, Some(before)) => {
                let i = find(before)?;
                (i.checked_sub(1), Some(i))
            }
            (None, None) => (items.len().checked_sub(1), None),
        };
        let position = fractional_index::between(
            lower.map(|i| items[i].position.as_str()),
            upper.map(|i| items[i].position.as_str()),
        )?;
        let item = repo::set_item_position(&txn, moving.id, position).await?;
        txn.commit().await?;
        Ok(item)
    }

    /// Lay the list out in `order` (titles not named keep their relative order
    /// after the named ones), assigning fresh evenly spaced positions.
    pub async fn reorder(
        &self,
        user_id: Uuid,
        list_id: Uuid,
        order: &[Uuid],
    ) -> Result<(), ListError> {
        self.get_list(user_id, list_id).await?;
        let txn = self.db.begin().await?;
        let mut items = repo::items_for_list(&txn, list_id).await?;
        let rank = |m: &Uuid| order.iter().position(|o| o == m).unwrap_or(usize::MAX);
        // Stable: unnamed items keep their existing relative order.
        items.sort_by_key(|i| rank(&i.media_id));
        let keys = fractional_index::spread(items.len());
        for (item, key) in items.into_iter().zip(keys) {
            if item.position != key {
                repo::set_item_position(&txn, item.id, key).await?;
            }
        }
        txn.commit().await?;
        Ok(())
    }

    /// A manual list's items as media, in position order, rendered from the
    /// catalog snapshot and overlaid with the user's library state.
    pub async fn list_media(
        &self,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<Vec<MediaWithUserState>, ListError> {
        self.get_list(user_id, list_id).await?;
        let items = repo::items_for_list(&self.db, list_id).await?;
        let media_ids: Vec<Uuid> = items.iter().map(|i| i.media_id).collect();
        let refs = catalog::primary_refs(&self.db, &media_ids).await?;
        let rows: std::collections::HashMap<Uuid, media::Model> = {
            use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
            media::Entity::find()
                .filter(media::Column::Id.is_in(media_ids.clone()))
                .all(&self.db)
                .await?
                .into_iter()
                .map(|m| (m.id, m))
                .collect()
        };
        let base = items
            .iter()
            .filter_map(|i| {
                let m = rows.get(&i.media_id)?;
                Some(MediaWithUserState::from_snapshot(m, refs.get(&i.media_id)?))
            })
            .collect();
        Ok(enrichment::enrich(&self.db, user_id, base).await?)
    }

    /// Evaluate a smart list's `filter_query` and return the matching media
    /// items, overlaid with the requesting user's library state.
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
        let filter: FilterQuery = match &list.filter_query {
            Some(raw) => serde_json::from_value(raw.clone())
                .map_err(|e| ListError::InvalidFilter(e.to_string()))?,
            None => {
                return Ok(Listing {
                    items: Vec::new(),
                    next_page: None,
                    unavailable: Vec::new(),
                    degraded: Vec::new(),
                });
            }
        };

        // Gather candidates from the chosen feed, one kind at a time, stopping
        // early once a feed runs out of pages.
        let feed = feed_for(&filter);
        let mut results = Vec::new();
        for kind in filter_kinds(&filter) {
            for page in 1..=SMART_LIST_PAGES {
                let result = metadata.feed(Provider::Tmdb, feed, kind, page).await;
                let stop = !matches!(&result, Ok(p) if p.has_next);
                results.push(result);
                if stop {
                    break;
                }
            }
        }

        let listing = media_service::merge_pages(1, results)?;
        let enriched = enrichment::enrich(&self.db, user_id, listing.items).await?;
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

/// The media kinds a smart-list filter targets — both when left unspecified.
fn filter_kinds(filter: &FilterQuery) -> Vec<MediaKind> {
    let Some(types) = &filter.media_type else {
        return vec![MediaKind::Movie, MediaKind::Show];
    };
    let mut kinds = Vec::new();
    for raw in types {
        let kind = match raw.as_str() {
            "movie" => MediaKind::Movie,
            "tv" | "show" => MediaKind::Show,
            _ => continue,
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    if kinds.is_empty() {
        vec![MediaKind::Movie, MediaKind::Show]
    } else {
        kinds
    }
}

/// The discovery feed a smart-list filter draws from, by `filter.sort`.
fn feed_for(filter: &FilterQuery) -> DiscoverCategory {
    match filter.sort.as_deref().unwrap_or("popular") {
        "top_rated" | "rating" | "score" => DiscoverCategory::TopRated,
        "trending" => DiscoverCategory::Trending,
        _ => DiscoverCategory::Popular,
    }
}

/// Test a media item against the smart-list filter predicates. `score_*` is
/// matched against the provider rating (a 0–10 vote average).
fn matches_filter(media: &MediaWithUserState, filter: &FilterQuery) -> bool {
    let genre_ok = filter.genre.as_ref().is_none_or(|genres| {
        genres.is_empty()
            || genres
                .iter()
                .any(|want| media.genres.iter().any(|g| g.eq_ignore_ascii_case(want)))
    });
    let year_ok = filter
        .year_min
        .is_none_or(|min| media.year.is_some_and(|y| y >= min))
        && filter
            .year_max
            .is_none_or(|max| media.year.is_some_and(|y| y <= max));
    let score_ok = filter
        .score_min
        .is_none_or(|min| media.rating.is_some_and(|r| r >= f64::from(min)))
        && filter
            .score_max
            .is_none_or(|max| media.rating.is_some_and(|r| r <= f64::from(max)));
    genre_ok && year_ok && score_ok
}
