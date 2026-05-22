use std::sync::Arc;

use cameo::{
    CameoClient, DetailProvider, DiscoveryProvider, RecommendationProvider, SearchProvider,
    SeasonProvider, TimeWindow, UnifiedSearchResult,
};
use sea_orm::DatabaseConnection;

use crate::pagination::{CursorParams, PaginatedResult, cursor_to_page, next_page_cursor};

use super::id::{MediaId, MediaKind};
use super::models::{EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};
use super::{MediaError, enrichment, mapping};

/// A discovery feed category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverCategory {
    /// Trending over the past week.
    Trending,
    /// All-time popular.
    Popular,
    /// Upcoming releases. Not yet supported — see [`MediaService::discover`].
    Upcoming,
    /// Highest rated.
    TopRated,
}

/// Media browsing service — delegates metadata lookups to cameo and overlays
/// the current user's watch state and streaming availability.
pub struct MediaService {
    db: DatabaseConnection,
    cameo: Arc<CameoClient>,
}

impl MediaService {
    /// Construct a new `MediaService`.
    pub fn new(db: DatabaseConnection, cameo: Arc<CameoClient>) -> Self {
        Self { db, cameo }
    }

    /// Full-text search across movies and TV shows.
    pub async fn search(
        &self,
        user_id: &str,
        query: &str,
        pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        let page = cursor_to_page(pagination.cursor.as_deref());
        let resp = self.cameo.search_multi(query, Some(page)).await?;

        let mut base = Vec::with_capacity(resp.results.len());
        for result in &resp.results {
            match result {
                UnifiedSearchResult::Movie(m) => base.push(mapping::movie_base(m)?),
                UnifiedSearchResult::TvShow(t) => base.push(mapping::tv_base(t)?),
                // People — and any future result kinds — are not trackable
                // media items, so they are skipped.
                _ => {}
            }
        }
        let items = enrichment::enrich(&self.db, user_id, base).await?;
        Ok(PaginatedResult::new(
            items,
            next_page_cursor(resp.page, resp.total_pages),
        ))
    }

    /// Full detail for one media item.
    pub async fn get_detail(
        &self,
        user_id: &str,
        id: &MediaId,
    ) -> Result<MediaDetails, MediaError> {
        let mut detail = match id.kind {
            MediaKind::Movie => {
                mapping::movie_details(&self.cameo.movie_details(id.numeric).await?)?
            }
            MediaKind::Tv => mapping::tv_details(&self.cameo.tv_show_details(id.numeric).await?)?,
        };
        detail.media = enrichment::enrich_one(&self.db, user_id, detail.media).await?;
        Ok(detail)
    }

    /// Fetch a batch of media items by id, concurrently, each overlaid with the
    /// user's watch state and streaming availability.
    ///
    /// Ids that cameo cannot resolve (e.g. a since-removed provider entry) are
    /// dropped silently, so one stale id never fails the whole batch. The
    /// result preserves the order of `ids` (minus any dropped entries).
    pub async fn get_many(
        &self,
        user_id: &str,
        ids: &[MediaId],
    ) -> Result<Vec<MediaWithUserState>, MediaError> {
        let fetches = ids.iter().map(|id| async move {
            match id.kind {
                MediaKind::Movie => self
                    .cameo
                    .movie_details(id.numeric)
                    .await
                    .ok()
                    .and_then(|d| mapping::movie_details(&d).ok())
                    .map(|d| d.media),
                MediaKind::Tv => self
                    .cameo
                    .tv_show_details(id.numeric)
                    .await
                    .ok()
                    .and_then(|d| mapping::tv_details(&d).ok())
                    .map(|d| d.media),
            }
        });
        let base: Vec<MediaWithUserState> = futures::future::join_all(fetches)
            .await
            .into_iter()
            .flatten()
            .collect();
        enrichment::enrich(&self.db, user_id, base).await
    }

    /// List the seasons of a TV show.
    pub async fn get_seasons(&self, id: &MediaId) -> Result<Vec<SeasonInfo>, MediaError> {
        ensure_tv(id)?;
        let details = self.cameo.tv_show_details(id.numeric).await?;
        // Fetch every season concurrently — cameo's unified API has no
        // single-call "list seasons", so each season is a `season_details` hit.
        let fetches = (1..=details.number_of_seasons)
            .map(|season| self.cameo.season_details(id.numeric, season));
        let seasons = futures::future::try_join_all(fetches).await?;
        Ok(seasons.iter().map(mapping::season_info).collect())
    }

    /// List the episodes of one season of a TV show.
    pub async fn get_episodes(
        &self,
        id: &MediaId,
        season: u32,
    ) -> Result<Vec<EpisodeInfo>, MediaError> {
        ensure_tv(id)?;
        let details = self.cameo.season_details(id.numeric, season).await?;
        Ok(details
            .episodes
            .iter()
            .map(|e| mapping::episode_info(e, season))
            .collect())
    }

    /// Recommendations derived from a given media item.
    pub async fn get_recommendations(
        &self,
        user_id: &str,
        id: &MediaId,
        pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        let page = cursor_to_page(pagination.cursor.as_deref());
        let (base, resp_page, total_pages) = match id.kind {
            MediaKind::Movie => {
                let r = self
                    .cameo
                    .movie_recommendations(id.numeric, Some(page))
                    .await?;
                (mapping::movies_base(&r.results)?, r.page, r.total_pages)
            }
            MediaKind::Tv => {
                let r = self
                    .cameo
                    .tv_recommendations(id.numeric, Some(page))
                    .await?;
                (mapping::tv_shows_base(&r.results)?, r.page, r.total_pages)
            }
        };
        let items = enrichment::enrich(&self.db, user_id, base).await?;
        Ok(PaginatedResult::new(
            items,
            next_page_cursor(resp_page, total_pages),
        ))
    }

    /// A discovery feed (trending / popular / top-rated).
    ///
    /// Discovery feeds are split per media kind upstream; when the caller does
    /// not specify a `media_type`, this defaults to movies.
    pub async fn discover(
        &self,
        user_id: &str,
        category: DiscoverCategory,
        media_type: Option<MediaKind>,
        pagination: &CursorParams,
    ) -> Result<PaginatedResult<MediaWithUserState>, MediaError> {
        let kind = media_type.unwrap_or(MediaKind::Movie);
        let page = cursor_to_page(pagination.cursor.as_deref());

        let (base, resp_page, total_pages) = match (category, kind) {
            (DiscoverCategory::Trending, MediaKind::Movie) => {
                let r = self
                    .cameo
                    .trending_movies(TimeWindow::Week, Some(page))
                    .await?;
                (mapping::movies_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::Trending, MediaKind::Tv) => {
                let r = self
                    .cameo
                    .trending_tv_shows(TimeWindow::Week, Some(page))
                    .await?;
                (mapping::tv_shows_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::Popular, MediaKind::Movie) => {
                let r = self.cameo.popular_movies(Some(page)).await?;
                (mapping::movies_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::Popular, MediaKind::Tv) => {
                let r = self.cameo.popular_tv_shows(Some(page)).await?;
                (mapping::tv_shows_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::TopRated, MediaKind::Movie) => {
                let r = self.cameo.top_rated_movies(Some(page)).await?;
                (mapping::movies_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::TopRated, MediaKind::Tv) => {
                let r = self.cameo.top_rated_tv_shows(Some(page)).await?;
                (mapping::tv_shows_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::Upcoming, _) => {
                // TODO(cameo): cameo's unified `DiscoveryProvider` exposes no
                // `upcoming` feed. Revisit once it gains upcoming-movies /
                // on-the-air-TV support; until then return an empty feed.
                tracing::warn!(
                    "/discover/upcoming requested but cameo has no upcoming API — returning empty feed"
                );
                return Ok(PaginatedResult::new(Vec::new(), None));
            }
        };
        let items = enrichment::enrich(&self.db, user_id, base).await?;
        Ok(PaginatedResult::new(
            items,
            next_page_cursor(resp_page, total_pages),
        ))
    }
}

/// Reject a non-TV id for season/episode lookups.
fn ensure_tv(id: &MediaId) -> Result<(), MediaError> {
    match id.kind {
        MediaKind::Tv => Ok(()),
        MediaKind::Movie => Err(MediaError::NotFound(format!("{id} is not a TV show"))),
    }
}
