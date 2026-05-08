use std::sync::Arc;

use cameo::CameoClient;
use cameo::unified::{
    DetailProvider, DiscoveryProvider, RecommendationProvider, SearchProvider, SeasonProvider,
    models::UnifiedSearchResult,
};
use sea_orm::DatabaseConnection;

use crate::cameo::tmdb_numeric_id;
use crate::pagination::{CursorParams, PaginatedResult};

use super::enrichment;
use super::filter::FilterQuery;
use super::models::{self, MediaWithUserState};
use super::MediaError;

pub struct MediaService {
    pub db: DatabaseConnection,
    pub cameo: Arc<CameoClient>,
}

impl MediaService {
    pub fn new(db: DatabaseConnection, cameo: Arc<CameoClient>) -> Self {
        Self { db, cameo }
    }

    /// Search Cameo for movies + TV shows matching `query` and enrich with
    /// the caller's watch state and streaming availability.
    pub async fn search(
        &self,
        user_id: &str,
        query: &str,
        _filter: &FilterQuery,
        pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        let page = cursor_to_page(pagination);
        let multi = self
            .cameo
            .search_multi(query, Some(page))
            .await
            .map_err(|e| MediaError::CameoError(e.to_string()))?;

        let total_pages = multi.total_pages;
        let mut items: Vec<MediaWithUserState> = Vec::new();
        for result in multi.results {
            match result {
                UnifiedSearchResult::Movie(m) => items.push(unified_movie_to_item(m)),
                UnifiedSearchResult::TvShow(t) => items.push(unified_tv_to_item(t)),
                _ => {}
            }
        }

        hydrate(&self.db, user_id, &mut items).await;

        let next_cursor = next_page_cursor(page, total_pages);
        Ok(PaginatedResult::new(items, next_cursor))
    }

    /// Look up full details for a single media item.
    pub async fn get_detail(
        &self,
        user_id: &str,
        cameo_id: &str,
    ) -> Result<MediaWithUserState, MediaError> {
        let tmdb =
            tmdb_numeric_id(cameo_id).ok_or_else(|| MediaError::NotFound(cameo_id.into()))?;

        if let Ok(movie) = self.cameo.movie_details(tmdb).await {
            let mut item = unified_movie_to_item(movie.movie.clone());
            item.overview = movie.movie.overview.clone();
            apply_enrichment(&self.db, user_id, &mut item).await;
            return Ok(item);
        }

        if let Ok(tv) = self.cameo.tv_show_details(tmdb).await {
            let mut item = unified_tv_to_item(tv.show.clone());
            item.overview = tv.show.overview.clone();
            apply_enrichment(&self.db, user_id, &mut item).await;
            return Ok(item);
        }

        Err(MediaError::NotFound(cameo_id.into()))
    }

    /// Returns a synthesised season list — `[{ season_number, name }]` —
    /// derived from `number_of_seasons` on the TV show details. Detailed
    /// per-season episode info is fetched on demand via `get_episodes`.
    pub async fn get_seasons(&self, cameo_id: &str) -> Result<Vec<serde_json::Value>, MediaError> {
        let tmdb =
            tmdb_numeric_id(cameo_id).ok_or_else(|| MediaError::NotFound(cameo_id.into()))?;
        let tv = self
            .cameo
            .tv_show_details(tmdb)
            .await
            .map_err(|e| MediaError::CameoError(e.to_string()))?;
        let seasons: Vec<serde_json::Value> = (1..=tv.number_of_seasons)
            .map(|n| {
                serde_json::json!({
                    "season_number": n,
                    "name": format!("Season {n}"),
                })
            })
            .collect();
        Ok(seasons)
    }

    pub async fn get_episodes(
        &self,
        cameo_id: &str,
        season: u32,
    ) -> Result<Vec<serde_json::Value>, MediaError> {
        let tmdb =
            tmdb_numeric_id(cameo_id).ok_or_else(|| MediaError::NotFound(cameo_id.into()))?;
        let season_details = self
            .cameo
            .season_details(tmdb, season)
            .await
            .map_err(|e| MediaError::CameoError(e.to_string()))?;
        Ok(season_details
            .episodes
            .iter()
            .map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null))
            .collect())
    }

    pub async fn get_recommendations(
        &self,
        user_id: &str,
        cameo_id: &str,
        pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        let tmdb =
            tmdb_numeric_id(cameo_id).ok_or_else(|| MediaError::NotFound(cameo_id.into()))?;
        let page = cursor_to_page(pagination);

        let mut items = Vec::new();
        let mut total_pages = 0_u32;
        if let Ok(recs) = self.cameo.movie_recommendations(tmdb, Some(page)).await {
            total_pages = recs.total_pages;
            for m in recs.results {
                items.push(unified_movie_to_item(m));
            }
        } else if let Ok(recs) = self.cameo.tv_recommendations(tmdb, Some(page)).await {
            total_pages = recs.total_pages;
            for t in recs.results {
                items.push(unified_tv_to_item(t));
            }
        }

        hydrate(&self.db, user_id, &mut items).await;
        Ok(PaginatedResult::new(
            items,
            next_page_cursor(page, total_pages),
        ))
    }

    /// Discover endpoints (`trending`, `popular`, `upcoming`, `top_rated`).
    /// `media_type` selects movie vs tv where applicable. `upcoming` falls
    /// back to `popular_movies` because the cameo SDK does not expose a
    /// dedicated upcoming endpoint today.
    pub async fn discover(
        &self,
        user_id: &str,
        category: &str,
        media_type: Option<&str>,
        pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        use cameo::core::config::TimeWindow;

        let page = cursor_to_page(pagination);
        let mt = media_type.unwrap_or("movie");
        let mut items: Vec<MediaWithUserState> = Vec::new();
        let total_pages: u32;

        match (category, mt) {
            ("trending", "tv") => {
                let res = self
                    .cameo
                    .trending_tv_shows(TimeWindow::Week, Some(page))
                    .await
                    .map_err(|e| MediaError::CameoError(e.to_string()))?;
                total_pages = res.total_pages;
                for t in res.results {
                    items.push(unified_tv_to_item(t));
                }
            }
            ("trending", _) => {
                let res = self
                    .cameo
                    .trending_movies(TimeWindow::Week, Some(page))
                    .await
                    .map_err(|e| MediaError::CameoError(e.to_string()))?;
                total_pages = res.total_pages;
                for m in res.results {
                    items.push(unified_movie_to_item(m));
                }
            }
            ("popular", "tv") | ("upcoming", "tv") => {
                let res = self
                    .cameo
                    .popular_tv_shows(Some(page))
                    .await
                    .map_err(|e| MediaError::CameoError(e.to_string()))?;
                total_pages = res.total_pages;
                for t in res.results {
                    items.push(unified_tv_to_item(t));
                }
            }
            ("popular", _) | ("upcoming", _) => {
                let res = self
                    .cameo
                    .popular_movies(Some(page))
                    .await
                    .map_err(|e| MediaError::CameoError(e.to_string()))?;
                total_pages = res.total_pages;
                for m in res.results {
                    items.push(unified_movie_to_item(m));
                }
            }
            ("top_rated", "tv") | ("top-rated", "tv") => {
                let res = self
                    .cameo
                    .top_rated_tv_shows(Some(page))
                    .await
                    .map_err(|e| MediaError::CameoError(e.to_string()))?;
                total_pages = res.total_pages;
                for t in res.results {
                    items.push(unified_tv_to_item(t));
                }
            }
            ("top_rated", _) | ("top-rated", _) => {
                let res = self
                    .cameo
                    .top_rated_movies(Some(page))
                    .await
                    .map_err(|e| MediaError::CameoError(e.to_string()))?;
                total_pages = res.total_pages;
                for m in res.results {
                    items.push(unified_movie_to_item(m));
                }
            }
            (other, _) => {
                return Err(MediaError::CameoError(format!(
                    "unknown discover category: {other}"
                )));
            }
        }

        hydrate(&self.db, user_id, &mut items).await;
        Ok(PaginatedResult::new(
            items,
            next_page_cursor(page, total_pages),
        ))
    }

    /// Fetch hydrated metadata for a list of cameo IDs (used by web hydration
    /// and the public `/media/batch` endpoint). Capped at 50 IDs per call.
    pub async fn batch_get(
        &self,
        user_id: &str,
        cameo_ids: &[String],
    ) -> Result<Vec<MediaWithUserState>, MediaError> {
        let mut items = Vec::new();
        for id in cameo_ids.iter().take(50) {
            if let Ok(item) = self.get_detail(user_id, id).await {
                items.push(item);
            }
        }
        Ok(items)
    }
}

async fn hydrate(
    db: &DatabaseConnection,
    user_id: &str,
    items: &mut [MediaWithUserState],
) {
    let cameo_ids: Vec<String> = items.iter().map(|i| i.cameo_media_id.clone()).collect();
    let enriched = enrichment::enrich_many(db, user_id, &cameo_ids).await;
    let mut by_id: std::collections::HashMap<String, _> = enriched
        .into_iter()
        .map(|e| (e.cameo_media_id.clone(), e))
        .collect();
    for item in items.iter_mut() {
        if let Some(state) = by_id.remove(&item.cameo_media_id) {
            item.watch_status = state.watch_status;
            item.score = state.score;
            item.is_favorite = state.is_favorite;
            item.availability = state.availability;
        }
    }
}

async fn apply_enrichment(
    db: &DatabaseConnection,
    user_id: &str,
    item: &mut MediaWithUserState,
) {
    if let Some(enriched) = enrichment::enrich_one(db, user_id, &item.cameo_media_id).await {
        item.watch_status = enriched.watch_status;
        item.score = enriched.score;
        item.is_favorite = enriched.is_favorite;
        item.availability = enriched.availability;
    }
}

fn cursor_to_page(p: &CursorParams) -> u32 {
    p.cursor
        .as_ref()
        .and_then(|c| c.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1)
}

fn next_page_cursor(current: u32, total: u32) -> Option<String> {
    if total == 0 || current < total {
        Some((current + 1).to_string())
    } else {
        None
    }
}

fn unified_movie_to_item(m: cameo::unified::UnifiedMovie) -> MediaWithUserState {
    let year = m
        .release_date
        .as_deref()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse::<i32>().ok());
    MediaWithUserState {
        cameo_media_id: m.provider_id.clone(),
        title: m.title,
        media_type: "movie".to_string(),
        year,
        overview: None,
        poster_url: m.poster_url,
        backdrop_url: m.backdrop_url,
        genres: m.genres.iter().map(|g| g.name().to_string()).collect(),
        rating: m.vote_average,
        watch_status: None,
        score: None,
        is_favorite: false,
        availability: models::AvailabilityInfo::default(),
    }
}

fn unified_tv_to_item(t: cameo::unified::UnifiedTvShow) -> MediaWithUserState {
    let year = t
        .first_air_date
        .as_deref()
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse::<i32>().ok());
    MediaWithUserState {
        cameo_media_id: t.provider_id.clone(),
        title: t.name,
        media_type: "tv".to_string(),
        year,
        overview: None,
        poster_url: t.poster_url,
        backdrop_url: t.backdrop_url,
        genres: t.genres.iter().map(|g| g.name().to_string()).collect(),
        rating: t.vote_average,
        watch_status: None,
        score: None,
        is_favorite: false,
        availability: models::AvailabilityInfo::default(),
    }
}
