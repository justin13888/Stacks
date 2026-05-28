use cameo::{CameoClient, DiscoveryProvider, TimeWindow};
use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::entity::{user_list, user_list_item};
use crate::media::filter::FilterQuery;
use crate::media::id::MediaKind;
use crate::media::models::MediaWithUserState;
use crate::media::{MediaError, enrichment, mapping};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateListRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub filter_query: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderItem {
    pub cameo_media_id: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

pub struct ListService {
    pub db: DatabaseConnection,
}

impl ListService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_list(
        &self,
        user_id: &str,
        req: CreateListRequest,
    ) -> Result<user_list::Model, ListError> {
        if req.name.trim().is_empty() {
            return Err(ListError::EmptyName);
        }
        let now = Utc::now();
        let model = user_list::ActiveModel {
            id: Set(Ulid::new().to_string()),
            user_id: Set(user_id.to_string()),
            name: Set(req.name),
            description: Set(req.description),
            is_smart: Set(req.is_smart),
            filter_query: Set(req.filter_query),
            sort_order: Set(None),
            created_at: Set(now.into()),
            updated_at: Set(now.into()),
        };
        repo::create_list(&self.db, model).await
    }

    pub async fn get_list(
        &self,
        user_id: &str,
        list_id: &str,
    ) -> Result<user_list::Model, ListError> {
        let list = repo::find_list_by_id(&self.db, list_id)
            .await?
            .ok_or(ListError::NotFound)?;
        if list.user_id != user_id {
            return Err(ListError::NotOwner);
        }
        Ok(list)
    }

    pub async fn get_user_lists(&self, user_id: &str) -> Result<Vec<user_list::Model>, ListError> {
        repo::find_lists_for_user(&self.db, user_id).await
    }

    pub async fn update_list(
        &self,
        user_id: &str,
        list_id: &str,
        req: UpdateListRequest,
    ) -> Result<user_list::Model, ListError> {
        let list = self.get_list(user_id, list_id).await?;
        let mut active: user_list::ActiveModel = list.into();
        if let Some(name) = req.name {
            active.name = Set(name);
        }
        if let Some(desc) = req.description {
            active.description = Set(Some(desc));
        }
        if let Some(fq) = req.filter_query {
            active.filter_query = Set(Some(fq));
        }
        active.updated_at = Set(Utc::now().into());
        repo::update_list(&self.db, active).await
    }

    pub async fn delete_list(&self, user_id: &str, list_id: &str) -> Result<(), ListError> {
        self.get_list(user_id, list_id).await?;
        repo::delete_list(&self.db, list_id).await.map(|_| ())
    }

    pub async fn add_item(
        &self,
        user_id: &str,
        list_id: &str,
        cameo_media_id: &str,
        notes: Option<String>,
    ) -> Result<user_list_item::Model, ListError> {
        self.get_list(user_id, list_id).await?;

        // Compute position: append after the last item.
        let items = repo::find_items_for_list(&self.db, list_id).await?;
        let last_pos = items.last().map(|i| i.position);
        let position = fractional_index::midpoint(last_pos, None);

        let model = user_list_item::ActiveModel {
            list_id: Set(list_id.to_string()),
            cameo_media_id: Set(cameo_media_id.to_string()),
            position: Set(position),
            added_at: Set(Utc::now().into()),
            notes: Set(notes),
        };
        repo::add_item(&self.db, model).await
    }

    pub async fn remove_item(
        &self,
        user_id: &str,
        list_id: &str,
        cameo_media_id: &str,
    ) -> Result<(), ListError> {
        self.get_list(user_id, list_id).await?;
        let removed = repo::remove_item(&self.db, list_id, cameo_media_id).await?;
        if removed {
            Ok(())
        } else {
            Err(ListError::NotFound)
        }
    }

    pub async fn reorder_items(
        &self,
        user_id: &str,
        list_id: &str,
        reorder: ReorderItem,
    ) -> Result<user_list_item::Model, ListError> {
        self.get_list(user_id, list_id).await?;

        // Resolve before/after positions from the actual items.
        let items = repo::find_items_for_list(&self.db, list_id).await?;
        let before_pos = reorder.before.as_deref().and_then(|bid| {
            items
                .iter()
                .find(|i| i.cameo_media_id == bid)
                .map(|i| i.position)
        });
        let after_pos = reorder.after.as_deref().and_then(|aid| {
            items
                .iter()
                .find(|i| i.cameo_media_id == aid)
                .map(|i| i.position)
        });
        let position = fractional_index::midpoint(before_pos, after_pos);

        repo::update_item_position(&self.db, list_id, &reorder.cameo_media_id, position).await
    }

    /// Evaluate a smart list's `filter_query` and return the matching media
    /// items, already overlaid with the requesting user's watch state and
    /// streaming availability.
    ///
    /// Candidates are drawn from a cameo discovery feed (selected by the
    /// filter's `sort` field) and then narrowed by the filter predicates. The
    /// `cameo` client is passed per-call so [`ListService::new`] stays
    /// database-only.
    pub async fn evaluate_smart_list(
        &self,
        cameo: &CameoClient,
        user_id: &str,
        list_id: &str,
    ) -> Result<Vec<MediaWithUserState>, ListError> {
        let list = self.get_list(user_id, list_id).await?;
        if !list.is_smart {
            return Err(ListError::NotSmart);
        }
        let filter: FilterQuery = match &list.filter_query {
            Some(raw) => serde_json::from_value(raw.clone())
                .map_err(|e| ListError::InvalidFilter(e.to_string()))?,
            None => return Ok(Vec::new()),
        };

        // Gather candidates from the chosen feed, one kind at a time, stopping
        // early once a feed runs out of pages.
        let mut base: Vec<MediaWithUserState> = Vec::new();
        for kind in filter_kinds(&filter) {
            for page in 1..=SMART_LIST_PAGES {
                let (items, total_pages) = fetch_feed(cameo, &filter, kind, page).await?;
                base.extend(items);
                if page >= total_pages {
                    break;
                }
            }
        }

        let enriched = enrichment::enrich(&self.db, user_id, base).await?;
        Ok(enriched
            .into_iter()
            .filter(|media| matches_filter(media, &filter))
            .collect())
    }
}

/// The media kinds a smart-list filter targets — both when left unspecified.
fn filter_kinds(filter: &FilterQuery) -> Vec<MediaKind> {
    let Some(types) = &filter.media_type else {
        return vec![MediaKind::Movie, MediaKind::Tv];
    };
    let mut kinds = Vec::new();
    for raw in types {
        let kind = match raw.as_str() {
            "movie" => MediaKind::Movie,
            "tv" => MediaKind::Tv,
            _ => continue,
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    if kinds.is_empty() {
        vec![MediaKind::Movie, MediaKind::Tv]
    } else {
        kinds
    }
}

/// Fetch one discovery-feed page for `kind`, returning the mapped base items
/// and the feed's total page count. The feed is selected by `filter.sort`.
async fn fetch_feed(
    cameo: &CameoClient,
    filter: &FilterQuery,
    kind: MediaKind,
    page: u32,
) -> Result<(Vec<MediaWithUserState>, u32), MediaError> {
    let sort = filter.sort.as_deref().unwrap_or("popular");
    Ok(match (sort, kind) {
        ("top_rated" | "rating" | "score", MediaKind::Movie) => {
            let r = cameo.top_rated_movies(Some(page)).await?;
            (mapping::movies_base(&r.results)?, r.total_pages)
        }
        ("top_rated" | "rating" | "score", MediaKind::Tv) => {
            let r = cameo.top_rated_tv_shows(Some(page)).await?;
            (mapping::tv_shows_base(&r.results)?, r.total_pages)
        }
        ("trending", MediaKind::Movie) => {
            let r = cameo.trending_movies(TimeWindow::Week, Some(page)).await?;
            (mapping::movies_base(&r.results)?, r.total_pages)
        }
        ("trending", MediaKind::Tv) => {
            let r = cameo
                .trending_tv_shows(TimeWindow::Week, Some(page))
                .await?;
            (mapping::tv_shows_base(&r.results)?, r.total_pages)
        }
        (_, MediaKind::Movie) => {
            let r = cameo.popular_movies(Some(page)).await?;
            (mapping::movies_base(&r.results)?, r.total_pages)
        }
        (_, MediaKind::Tv) => {
            let r = cameo.popular_tv_shows(Some(page)).await?;
            (mapping::tv_shows_base(&r.results)?, r.total_pages)
        }
    })
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
    let beam_ok = filter
        .available_on_beam
        .is_none_or(|want| media.availability.available == want);
    genre_ok && year_ok && score_ok && beam_ok
}
