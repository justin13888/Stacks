//! Media browsing: live metadata through [`MetadataProvider`], with the
//! catalog snapshot as the fallback whenever a provider fails.
//!
//! Every provider failure arrives already classified as a [`MetadataError`].
//! This module never discards one: a read either falls back to the snapshot
//! (marked stale), reports the failure in a [`Listing`]'s `degraded` list, or
//! returns it.

use std::sync::Arc;

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::media;
use crate::pagination::{CursorParams, cursor_to_page};

use super::catalog;
use super::id::{MediaId, MediaKind, Provider};
use super::models::{
    EpisodeInfo, EpisodeList, Freshness, Listing, MediaDetails, MediaWithUserState, SeasonInfo,
    SeasonList,
};
use super::provider::{DiscoverCategory, MetadataPage, MetadataProvider};
use super::{MediaError, MetadataError, enrichment};

/// Media browsing service — delegates metadata lookups to the
/// [`MetadataProvider`], keeps the catalog snapshot, and overlays the current
/// user's library state.
pub struct MediaService {
    db: DatabaseConnection,
    metadata: Arc<dyn MetadataProvider>,
}

impl MediaService {
    /// Construct a new `MediaService`.
    pub fn new(db: DatabaseConnection, metadata: Arc<dyn MetadataProvider>) -> Self {
        Self { db, metadata }
    }

    /// Search movies and shows on every provider at once.
    ///
    /// A provider that fails is reported in `degraded` while the other's
    /// results still come back; only when every provider fails is the search
    /// an error.
    pub async fn search(
        &self,
        user_id: Uuid,
        query: &str,
        pagination: &CursorParams,
    ) -> Result<Listing<MediaWithUserState>, MediaError> {
        let page = cursor_to_page(pagination.cursor.as_deref());
        let (tmdb, anilist) = futures::join!(
            self.metadata.search(Provider::Tmdb, query, page),
            self.metadata.search(Provider::AniList, query, page),
        );
        let listing = merge_pages(page, vec![tmdb, anilist])?;
        self.enrich_listing(user_id, listing).await
    }

    /// Full detail for one media item.
    ///
    /// A successful fetch refreshes the title's snapshot when it is in the
    /// catalog. When the provider fails and a snapshot exists, the snapshot
    /// is served instead, marked stale. That includes a payload cameo cannot
    /// deserialize, such as an explicit `null` in a field it requires (cameo
    /// gap, Stacks#14).
    pub async fn get_detail(
        &self,
        user_id: Uuid,
        id: &MediaId,
    ) -> Result<MediaDetails, MediaError> {
        let stored = catalog::find_by_ref(&self.db, id).await?;
        let mut detail = match self.metadata.details(id).await {
            Ok(title) => {
                if let Some(row) = &stored {
                    self.refresh_snapshot(row, &title.snapshot, &title.seasons)
                        .await;
                }
                title.detail
            }
            Err(e) => match &stored {
                Some(row) => {
                    tracing::warn!(media_id = %id, error = %e, "serving the stale snapshot");
                    MediaDetails::from_snapshot(row, id)
                }
                None => return Err(e.into()),
            },
        };
        detail.media = enrichment::enrich_one(&self.db, user_id, detail.media).await?;
        Ok(detail)
    }

    /// The catalog row for `id`, fetching and storing a snapshot the first
    /// time a title is seen.
    ///
    /// A title the provider reports as missing is an error. Any other
    /// provider failure stores a placeholder snapshot instead, so tracking a
    /// title never fails because an upstream is down; the placeholder records
    /// the error for the `metadata_refresh` job.
    pub async fn ensure_in_catalog(&self, id: &MediaId) -> Result<media::Model, MediaError> {
        if let Some(existing) = catalog::find_by_ref(&self.db, id).await? {
            return Ok(existing);
        }
        match self.metadata.details(id).await {
            Ok(title) => {
                let row = catalog::upsert_snapshot(&self.db, &title.snapshot).await?;
                catalog::upsert_seasons(&self.db, row.id, &title.seasons).await?;
                Ok(row)
            }
            Err(e) => match MediaError::from(e) {
                MediaError::NotFound(msg) => Err(MediaError::NotFound(msg)),
                e => {
                    tracing::warn!(
                        media_id = %id,
                        error = %e,
                        "provider unavailable; storing a placeholder snapshot"
                    );
                    Ok(catalog::insert_placeholder(&self.db, id, &e.to_string()).await?)
                }
            },
        }
    }

    /// The seasons of a show, from its detail call; the stored seasons when
    /// the provider fails.
    pub async fn get_seasons(&self, id: &MediaId) -> Result<SeasonList, MediaError> {
        ensure_show(id)?;
        let stored = catalog::find_by_ref(&self.db, id).await?;
        match self.metadata.details(id).await {
            Ok(title) => {
                if let Some(row) = &stored {
                    self.refresh_snapshot(row, &title.snapshot, &title.seasons)
                        .await;
                }
                Ok(SeasonList {
                    seasons: title.seasons,
                    freshness: Freshness::live(),
                })
            }
            Err(e) => {
                let Some(row) = stored else {
                    return Err(e.into());
                };
                let seasons = catalog::seasons_of(&self.db, row.id).await?;
                if seasons.is_empty() {
                    return Err(e.into());
                }
                tracing::warn!(media_id = %id, error = %e, "serving stale seasons");
                Ok(SeasonList {
                    seasons: seasons.iter().map(SeasonInfo::from_snapshot).collect(),
                    freshness: Freshness::stale(row.fetched_at),
                })
            }
        }
    }

    /// The episodes of one season of a show; the stored episodes when the
    /// provider fails.
    pub async fn get_episodes(&self, id: &MediaId, season: u32) -> Result<EpisodeList, MediaError> {
        ensure_show(id)?;
        let stored = catalog::find_by_ref(&self.db, id).await?;
        match self.metadata.season(id, season).await {
            Ok(details) => {
                if let Some(row) = &stored
                    && let Err(e) = catalog::upsert_season_episodes(
                        &self.db,
                        row.id,
                        &details.season,
                        &details.episodes,
                    )
                    .await
                {
                    tracing::warn!(media_id = %id, error = %e, "failed to store episodes");
                }
                Ok(EpisodeList {
                    episodes: details.episodes,
                    freshness: Freshness::live(),
                })
            }
            Err(e) => {
                let Some(row) = stored else {
                    return Err(e.into());
                };
                let number = i32::try_from(season)
                    .map_err(|_| MediaError::NotFound(format!("{id} season {season}")))?;
                let episodes = catalog::episodes_of(&self.db, row.id, number).await?;
                if episodes.is_empty() {
                    return Err(e.into());
                }
                tracing::warn!(media_id = %id, season, error = %e, "serving stale episodes");
                Ok(EpisodeList {
                    episodes: episodes
                        .iter()
                        .map(|ep| EpisodeInfo::from_snapshot(season, ep))
                        .collect(),
                    freshness: Freshness::stale(row.fetched_at),
                })
            }
        }
    }

    /// Recommendations derived from a given media item. There is no snapshot
    /// of recommendations, so a provider failure is returned as such.
    pub async fn get_recommendations(
        &self,
        user_id: Uuid,
        id: &MediaId,
        pagination: &CursorParams,
    ) -> Result<Listing<MediaWithUserState>, MediaError> {
        let page = cursor_to_page(pagination.cursor.as_deref());
        let result = self.metadata.recommendations(id, page).await;
        let listing = merge_pages(page, vec![result])?;
        self.enrich_listing(user_id, listing).await
    }

    /// A TMDB discovery feed (trending / popular / top-rated).
    ///
    /// Discovery feeds are split per media kind upstream; when the caller does
    /// not specify a kind, this defaults to movies.
    pub async fn discover(
        &self,
        user_id: Uuid,
        category: DiscoverCategory,
        media_type: Option<MediaKind>,
        pagination: &CursorParams,
    ) -> Result<Listing<MediaWithUserState>, MediaError> {
        let kind = media_type.unwrap_or(MediaKind::Movie);
        let page = cursor_to_page(pagination.cursor.as_deref());
        let result = self
            .metadata
            .feed(Provider::Tmdb, category, kind, page)
            .await;
        let listing = merge_pages(page, vec![result])?;
        self.enrich_listing(user_id, listing).await
    }

    /// Store a fresh snapshot. A storage failure is logged, not returned: the
    /// caller already holds the live data it asked for.
    async fn refresh_snapshot(
        &self,
        row: &media::Model,
        snapshot: &catalog::MediaSnapshot,
        seasons: &[SeasonInfo],
    ) {
        let stored = match catalog::upsert_snapshot(&self.db, snapshot).await {
            Ok(stored) => catalog::upsert_seasons(&self.db, stored.id, seasons).await,
            Err(e) => Err(e),
        };
        if let Err(e) = stored {
            tracing::warn!(media_id = %row.id, error = %e, "failed to refresh catalog snapshot");
        }
    }

    async fn enrich_listing(
        &self,
        user_id: Uuid,
        listing: Listing<MediaWithUserState>,
    ) -> Result<Listing<MediaWithUserState>, MediaError> {
        let items = enrichment::enrich(&self.db, user_id, listing.items).await?;
        Ok(Listing { items, ..listing })
    }
}

/// Merge the pages of several provider calls made for the same page number.
///
/// Failed calls go to `degraded`; the listing is an error only when every
/// call failed, and then it is the first call's error.
pub fn merge_pages(
    page: u32,
    results: Vec<Result<MetadataPage<MediaWithUserState>, MetadataError>>,
) -> Result<Listing<MediaWithUserState>, MediaError> {
    let mut items = Vec::new();
    let mut unavailable = Vec::new();
    let mut degraded = Vec::new();
    let mut has_next = false;
    let mut any_ok = false;
    for result in results {
        match result {
            Ok(p) => {
                any_ok = true;
                has_next |= p.has_next;
                items.extend(p.items);
                unavailable.extend(p.unavailable);
            }
            Err(e) => degraded.push(e),
        }
    }
    if !any_ok && !degraded.is_empty() {
        return Err(degraded.swap_remove(0).into());
    }
    Ok(Listing {
        items,
        next_cursor: if has_next {
            crate::pagination::next_page_cursor(page, page.saturating_add(1))
        } else {
            None
        },
        unavailable,
        degraded,
    })
}

/// Reject a non-show id for season/episode lookups.
fn ensure_show(id: &MediaId) -> Result<(), MediaError> {
    match id.kind {
        MediaKind::Show => Ok(()),
        MediaKind::Movie => Err(MediaError::NotFound(format!("{id} is not a show"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{MetadataErrorKind, Operation, UnavailableCause};

    fn page(items: usize, has_next: bool) -> MetadataPage<MediaWithUserState> {
        MetadataPage {
            items: (0..items)
                .map(|i| MediaWithUserState {
                    cameo_media_id: format!("tmdb:movie:{i}"),
                    title: format!("t{i}"),
                    kind: MediaKind::Movie,
                    year: None,
                    overview: None,
                    poster_url: None,
                    backdrop_url: None,
                    genres: Vec::new(),
                    rating: None,
                    watch_status: None,
                    rewatching: false,
                    score: None,
                    is_favorite: false,
                })
                .collect(),
            page: 1,
            has_next,
            unavailable: vec!["tmdb:9".into()],
        }
    }

    fn down(provider: Provider) -> MetadataError {
        MetadataError::new(
            provider,
            Operation::Search,
            MetadataErrorKind::Unavailable(UnavailableCause::Status(503)),
        )
    }

    #[test]
    fn one_failed_provider_degrades_the_listing() {
        let listing = merge_pages(1, vec![Ok(page(2, true)), Err(down(Provider::AniList))])
            .expect("one provider answered");
        assert_eq!(listing.items.len(), 2);
        assert!(listing.is_partial());
        assert_eq!(listing.degraded, vec![down(Provider::AniList)]);
        assert_eq!(listing.unavailable, vec!["tmdb:9".to_owned()]);
        assert!(listing.next_cursor.is_some());
    }

    #[test]
    fn every_provider_failing_is_the_first_error() {
        let err = merge_pages(
            1,
            vec![Err(down(Provider::Tmdb)), Err(down(Provider::AniList))],
        )
        .expect_err("nothing answered");
        assert!(matches!(err, MediaError::Metadata(e) if e.provider == Provider::Tmdb));
    }

    #[test]
    fn last_page_has_no_cursor() {
        let listing = merge_pages(3, vec![Ok(page(1, false))]).expect("ok");
        assert!(listing.next_cursor.is_none());
    }
}
