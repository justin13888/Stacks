use std::sync::Arc;

use cameo::{
    DetailProvider, DiscoveryProvider, RecommendationProvider, SearchProvider, SeasonProvider,
    TimeWindow, UnifiedSearchResult,
};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::media;
use crate::pagination::{CursorParams, PaginatedResult, cursor_to_page, next_page_cursor};

use super::catalog::{self, MediaSnapshot};
use super::id::{MediaId, MediaKind, Provider};
use super::models::{EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};
use super::{MediaError, enrichment, mapping};

/// A discovery feed category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverCategory {
    /// Trending over the past week.
    Trending,
    /// All-time popular.
    Popular,
    /// Highest rated.
    TopRated,
}

/// Media browsing service — delegates metadata lookups to cameo, keeps the
/// catalog snapshot, and overlays the current user's library state.
pub struct MediaService {
    db: DatabaseConnection,
    cameo: Arc<super::MediaClient>,
}

impl MediaService {
    /// Construct a new `MediaService`.
    pub fn new(db: DatabaseConnection, cameo: Arc<super::MediaClient>) -> Self {
        Self { db, cameo }
    }

    /// Full-text search across movies and shows.
    pub async fn search(
        &self,
        user_id: Uuid,
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

    /// Fetch full detail and a catalog snapshot for one id, routing on its
    /// provider.
    ///
    /// cameo's unified facade always prefers TMDB when it is configured, so an
    /// AniList id has to be sent to the AniList client explicitly or it would
    /// resolve against TMDB and return an unrelated title.
    async fn fetch(&self, id: &MediaId) -> Result<(MediaDetails, MediaSnapshot), MediaError> {
        Ok(match (id.provider, id.kind) {
            (Provider::Tmdb, MediaKind::Movie) => {
                let d = self.cameo.movie_details(id.numeric).await?;
                (mapping::movie_details(&d)?, mapping::movie_snapshot(&d)?)
            }
            (Provider::Tmdb, MediaKind::Show) => {
                let d = self.cameo.tv_show_details(id.numeric).await?;
                (mapping::tv_details(&d)?, mapping::show_snapshot(&d)?)
            }
            (Provider::AniList, MediaKind::Movie) => {
                let d = self.cameo.anilist().movie_details(id.numeric).await?;
                (mapping::movie_details(&d)?, mapping::movie_snapshot(&d)?)
            }
            (Provider::AniList, MediaKind::Show) => {
                let d = self.cameo.anilist().tv_show_details(id.numeric).await?;
                (mapping::tv_details(&d)?, mapping::show_snapshot(&d)?)
            }
        })
    }

    /// Full detail for one media item. A successful fetch also refreshes the
    /// title's catalog snapshot when it is already in the catalog.
    pub async fn get_detail(
        &self,
        user_id: Uuid,
        id: &MediaId,
    ) -> Result<MediaDetails, MediaError> {
        let (mut detail, snapshot) = self.fetch(id).await?;
        if catalog::find_by_ref(&self.db, id).await?.is_some()
            && let Err(e) = catalog::upsert_snapshot(&self.db, &snapshot).await
        {
            tracing::warn!(media_id = %id, error = %e, "failed to refresh catalog snapshot");
        }
        detail.media = enrichment::enrich_one(&self.db, user_id, detail.media).await?;
        Ok(detail)
    }

    /// The catalog row for `id`, fetching and storing a snapshot the first
    /// time a title is seen.
    ///
    /// A title the provider reports as missing is an error. Any other
    /// provider failure stores a placeholder snapshot instead, so tracking a
    /// title never fails because an upstream is down; the placeholder records
    /// the error for a later refresh.
    pub async fn ensure_in_catalog(&self, id: &MediaId) -> Result<media::Model, MediaError> {
        if let Some(existing) = catalog::find_by_ref(&self.db, id).await? {
            return Ok(existing);
        }
        match self.fetch(id).await {
            Ok((_, snapshot)) => Ok(catalog::upsert_snapshot(&self.db, &snapshot).await?),
            Err(MediaError::NotFound(msg)) => Err(MediaError::NotFound(msg)),
            Err(e) => {
                tracing::warn!(
                    media_id = %id,
                    error = %e,
                    "provider unavailable; storing a placeholder snapshot"
                );
                Ok(catalog::insert_placeholder(&self.db, id, &e.to_string()).await?)
            }
        }
    }

    /// List the seasons of a show.
    pub async fn get_seasons(&self, id: &MediaId) -> Result<Vec<SeasonInfo>, MediaError> {
        ensure_show(id)?;
        let details = self.cameo.tv_show_details(id.numeric).await?;
        // Fetch every season concurrently — cameo's unified API has no
        // single-call "list seasons", so each season is a `season_details` hit.
        let fetches = (1..=details.number_of_seasons)
            .map(|season| self.cameo.season_details(id.numeric, season));
        let seasons = futures::future::try_join_all(fetches).await?;
        Ok(seasons.iter().map(mapping::season_info).collect())
    }

    /// List the episodes of one season of a show.
    pub async fn get_episodes(
        &self,
        id: &MediaId,
        season: u32,
    ) -> Result<Vec<EpisodeInfo>, MediaError> {
        ensure_show(id)?;
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
        user_id: Uuid,
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
            MediaKind::Show => {
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
    /// not specify a kind, this defaults to movies.
    pub async fn discover(
        &self,
        user_id: Uuid,
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
            (DiscoverCategory::Trending, MediaKind::Show) => {
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
            (DiscoverCategory::Popular, MediaKind::Show) => {
                let r = self.cameo.popular_tv_shows(Some(page)).await?;
                (mapping::tv_shows_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::TopRated, MediaKind::Movie) => {
                let r = self.cameo.top_rated_movies(Some(page)).await?;
                (mapping::movies_base(&r.results)?, r.page, r.total_pages)
            }
            (DiscoverCategory::TopRated, MediaKind::Show) => {
                let r = self.cameo.top_rated_tv_shows(Some(page)).await?;
                (mapping::tv_shows_base(&r.results)?, r.page, r.total_pages)
            }
        };
        let items = enrichment::enrich(&self.db, user_id, base).await?;
        Ok(PaginatedResult::new(
            items,
            next_page_cursor(resp_page, total_pages),
        ))
    }
}

/// Reject a non-show id for season/episode lookups.
fn ensure_show(id: &MediaId) -> Result<(), MediaError> {
    match id.kind {
        MediaKind::Show => Ok(()),
        MediaKind::Movie => Err(MediaError::NotFound(format!("{id} is not a show"))),
    }
}
