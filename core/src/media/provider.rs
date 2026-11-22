//! The metadata seam: [`MetadataProvider`], and [`CameoProvider`], its only
//! production implementation.
//!
//! Every metadata call in Stacks goes through [`MetadataProvider`]. The trait
//! speaks only Stacks types, so callers and tests never see cameo, and a mock
//! provider can drive any service. [`CameoProvider`] is the adapter over
//! cameo: together with [`mapping`](super::mapping) it is the only code
//! allowed to name `cameo::` types (a test in
//! `core/tests/metadata_fault_injection.rs` enforces that).
//!
//! The adapter bounds and classifies every call:
//!
//! - explicit connect and request timeouts per provider, plus a deadline over
//!   the whole call including cameo's retries;
//! - cameo's [`RetryPolicy`] (bounded, jittered; 429 and 5xx and transport
//!   errors only, honouring `Retry-After`) and a per-provider [`RateLimit`],
//!   plus a limiter of Stacks' own for its own AniList queries;
//! - a [`CircuitBreaker`] per provider;
//! - routing on [`MediaId::provider`]: an AniList id never reaches TMDB, and
//!   a capability the provider lacks is [`MetadataErrorKind::Unsupported`]
//!   without any request;
//! - one [`MetadataError`] class per failure, counted in
//!   `stacks_metadata_errors_total{provider,operation,class}`, with every call
//!   timed in `stacks_metadata_request_duration_seconds{provider,operation,outcome}`.

use std::time::Duration;

use cameo::providers::anilist::{AnimeSort, AnimeStatus};
use cameo::providers::tmdb::TvSortBy;
use cameo::{
    AniListClient, AniListConfig, DetailProvider, DiscoveryProvider, MatchKind, ProviderError,
    RateLimit, RecommendationProvider, RetryPolicy, SearchProvider, SeasonProvider, TimeWindow,
    TmdbClient, TmdbConfig, UnifiedSearchResult,
};
use futures::future::BoxFuture;
use thiserror::Error;

use crate::entity::enums::Category;

use super::breaker::{BreakerState, CircuitBreaker};
use super::catalog::MediaSnapshot;
use super::error::{MetadataError, MetadataErrorKind, Operation, UnavailableCause};
use super::id::{MediaId, MediaKind, Provider};
use super::models::{EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};
use super::pacer::Pacer;
use super::{anilist, mapping};

mod call;
mod graphql;
mod message;
mod settings;

pub use settings::{ANILIST_URL, ProviderSettings, RateSettings, RetrySettings};

use message::sanitize;

/// A discovery feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverCategory {
    /// Trending over the past week.
    Trending,
    /// All-time popular.
    Popular,
    /// Highest rated.
    TopRated,
    /// Currently releasing, most popular first. AniList only.
    Airing,
    /// Scripted shows from one region, most popular first. TMDB shows only.
    RegionalDrama(DramaRegion),
}

/// A region whose scripted shows form a drama feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DramaRegion {
    Korea,
    Japan,
    /// Mainland China and Hong Kong.
    China,
    Taiwan,
    Thailand,
}

impl DramaRegion {
    /// The ISO 3166-1 origin countries the feed covers.
    pub fn origin_countries(self) -> &'static [&'static str] {
        match self {
            Self::Korea => &["KR"],
            Self::Japan => &["JP"],
            Self::China => &["CN", "HK"],
            Self::Taiwan => &["TW"],
            Self::Thailand => &["TH"],
        }
    }

    /// The category every title in the feed carries.
    pub fn category(self) -> Category {
        match self {
            Self::Korea => Category::KDrama,
            Self::Japan => Category::JDrama,
            Self::China => Category::CDrama,
            Self::Taiwan => Category::TwDrama,
            Self::Thailand => Category::ThDrama,
        }
    }
}

/// TMDB genres that make a show something other than a scripted drama:
/// Animation, Reality, Talk, News and Documentary, the same set
/// [`derive_category`](super::category::derive_category) excludes.
const NON_DRAMA_TMDB_GENRES: [u64; 5] = [16, 10764, 10767, 10763, 99];

/// One page of results from one provider call.
#[derive(Debug, Clone)]
pub struct MetadataPage<T> {
    pub items: Vec<T>,
    /// The 1-indexed page this is.
    pub page: u32,
    /// Whether another page follows.
    pub has_next: bool,
    /// Upstream entries that could not be mapped into items, by the ref the
    /// provider gave them.
    pub unavailable: Vec<String>,
}

/// Everything one detail call yields.
#[derive(Debug)]
pub struct TitleDetails {
    /// The detail view.
    pub detail: MediaDetails,
    /// The catalog snapshot to store.
    pub snapshot: MediaSnapshot,
    /// Season summaries (empty for movies; an AniList show's one season is
    /// synthesised, see [`anilist`]).
    pub seasons: Vec<SeasonInfo>,
}

/// One season with its episodes (and an AniList show's next to air).
#[derive(Debug)]
pub struct SeasonDetails {
    pub season: SeasonInfo,
    pub episodes: Vec<EpisodeInfo>,
    pub next_airing: Option<anilist::NextAiring>,
}

/// The only path from Stacks to a metadata provider.
///
/// Implementations route on the id's provider, never panic, and return every
/// failure as a classified [`MetadataError`].
///
/// The methods return boxed futures so the trait stays dyn-compatible
/// (`Arc<dyn MetadataProvider>`); an implementation writes
/// `Box::pin(async move { … })`.
pub trait MetadataProvider: Send + Sync {
    /// Free-text search of one provider's movies and shows. People are not
    /// trackable and are left out.
    fn search<'a>(
        &'a self,
        provider: Provider,
        query: &'a str,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>>;

    /// Full detail for one title.
    fn details<'a>(&'a self, id: &'a MediaId) -> MetadataFuture<'a, TitleDetails>;

    /// One season of a show, with its episodes.
    fn season<'a>(&'a self, id: &'a MediaId, season: u32) -> MetadataFuture<'a, SeasonDetails>;

    /// Recommendations derived from one title.
    fn recommendations<'a>(
        &'a self,
        id: &'a MediaId,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>>;

    /// A discovery feed of one provider.
    fn feed<'a>(
        &'a self,
        provider: Provider,
        feed: DiscoverCategory,
        kind: MediaKind,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>>;

    /// The AniList title MyAnimeList knows as `mal_id`.
    ///
    /// [`MetadataErrorKind::NotFound`] when AniList links no title to it. An
    /// implementation that cannot look titles up this way answers
    /// [`MetadataErrorKind::Unsupported`], which is the default.
    fn anilist_by_mal_id(&self, mal_id: i64) -> MetadataFuture<'_, MediaId> {
        let _ = mal_id;
        Box::pin(async {
            Err(MetadataError::new(
                Provider::AniList,
                Operation::Extras,
                MetadataErrorKind::Unsupported,
            ))
        })
    }

    /// The circuit-breaker state of each provider this implementation calls,
    /// for readiness reporting. Empty for an implementation without breakers.
    fn circuits(&self) -> Vec<(Provider, BreakerState)> {
        Vec::new()
    }
}

/// The future every [`MetadataProvider`] method returns.
pub type MetadataFuture<'a, T> = BoxFuture<'a, Result<T, MetadataError>>;

/// The adapter could not be built from its configuration.
#[derive(Debug, Error)]
#[error("metadata provider configuration: {0}")]
pub struct ProviderConfigError(String);

/// [`MetadataProvider`] over cameo's TMDB and AniList clients.
pub struct CameoProvider {
    tmdb: TmdbClient,
    anilist: AniListClient,
    /// Stacks' own AniList requests (the extras and episode-count queries).
    extras_http: reqwest::Client,
    /// Paces `extras_http`, which cameo's rate limiter does not see.
    extras_rate: Pacer,
    anilist_url: String,
    tmdb_breaker: CircuitBreaker,
    anilist_breaker: CircuitBreaker,
    call_deadline: Duration,
}

impl CameoProvider {
    /// Build the adapter. TMDB needs a credential (a v4 read token, or a v3
    /// key, which is shorter); AniList needs none.
    pub fn new(
        tmdb_credential: &str,
        settings: &ProviderSettings,
    ) -> Result<Self, ProviderConfigError> {
        let retry = RetryPolicy::new(
            settings.retry.max_retries,
            settings.retry.base_delay,
            settings.retry.max_delay,
        );
        let rate = |r: RateSettings| RateLimit::per(r.max_requests, r.per);

        // v4 read-access tokens are JWTs; a 32-hex-char string is a v3 key.
        let mut tmdb_config = if tmdb_credential.len() <= 40 && !tmdb_credential.contains('.') {
            TmdbConfig::new_v3(tmdb_credential)
        } else {
            TmdbConfig::new(tmdb_credential)
        };
        tmdb_config = tmdb_config
            .with_connect_timeout(settings.connect_timeout)
            .with_request_timeout(settings.request_timeout)
            .with_retry(retry)
            .with_rate_limit(rate(settings.tmdb_rate));
        if let Some(lang) = &settings.tmdb_language {
            tmdb_config = tmdb_config.with_language(lang.clone());
        }
        if let Some(url) = &settings.tmdb_base_url {
            tmdb_config = tmdb_config.with_base_url(url.clone());
        }
        let tmdb = TmdbClient::new(tmdb_config).map_err(|e| ProviderConfigError(e.to_string()))?;

        let anilist_url = settings
            .anilist_base_url
            .clone()
            .unwrap_or_else(|| ANILIST_URL.to_owned());
        let anilist = AniListClient::new(
            AniListConfig::new_with_base_url(anilist_url.clone())
                .with_connect_timeout(settings.connect_timeout)
                .with_request_timeout(settings.request_timeout)
                .with_retry(retry)
                .with_rate_limit(rate(settings.anilist_rate)),
        )
        .map_err(|e| ProviderConfigError(e.to_string()))?;

        let extras_http = reqwest::Client::builder()
            .user_agent(concat!("stacks/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(settings.connect_timeout)
            .timeout(settings.request_timeout)
            .build()
            .map_err(|e| ProviderConfigError(e.to_string()))?;

        Ok(Self {
            tmdb,
            anilist,
            extras_http,
            extras_rate: Pacer::new(settings.anilist_extras_rate),
            anilist_url,
            tmdb_breaker: CircuitBreaker::new(Provider::Tmdb, settings.breaker),
            anilist_breaker: CircuitBreaker::new(Provider::AniList, settings.breaker),
            call_deadline: settings.call_deadline,
        })
    }
}

impl MetadataProvider for CameoProvider {
    fn search<'a>(
        &'a self,
        provider: Provider,
        query: &'a str,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(self.search_impl(provider, query, page))
    }

    fn details<'a>(&'a self, id: &'a MediaId) -> MetadataFuture<'a, TitleDetails> {
        Box::pin(self.details_impl(id))
    }

    fn season<'a>(&'a self, id: &'a MediaId, season: u32) -> MetadataFuture<'a, SeasonDetails> {
        Box::pin(self.season_impl(id, season))
    }

    fn recommendations<'a>(
        &'a self,
        id: &'a MediaId,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(self.recommendations_impl(id, page))
    }

    fn feed<'a>(
        &'a self,
        provider: Provider,
        feed: DiscoverCategory,
        kind: MediaKind,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(self.feed_impl(provider, feed, kind, page))
    }

    fn anilist_by_mal_id(&self, mal_id: i64) -> MetadataFuture<'_, MediaId> {
        Box::pin(async move {
            // AniList's ids are positive `Int`s: anything else names no
            // title, and nothing is sent for it.
            let Some(mal_id) = i32::try_from(mal_id).ok().filter(|n| *n > 0) else {
                return Err(MetadataError::new(
                    Provider::AniList,
                    Operation::Extras,
                    MetadataErrorKind::NotFound,
                ));
            };
            self.call(Provider::AniList, Operation::Extras, async {
                let found = self
                    .anilist
                    .find_by_mal_id(mal_id)
                    .await
                    .map_err(|e| classify(e.into()))?
                    .ok_or(MetadataErrorKind::NotFound)?;
                let kind = match found.kind {
                    MatchKind::Movie => MediaKind::Movie,
                    _ => MediaKind::Show,
                };
                MediaId::from_cameo(&found.id.to_string(), kind)
                    .map_err(|e| MetadataErrorKind::BadUpstreamResponse(e.to_string()))
            })
            .await
        })
    }

    fn circuits(&self) -> Vec<(Provider, BreakerState)> {
        [Provider::Tmdb, Provider::AniList]
            .into_iter()
            .map(|p| (p, self.circuit_state(p)))
            .collect()
    }
}

impl CameoProvider {
    async fn search_impl(
        &self,
        provider: Provider,
        query: &str,
        page: u32,
    ) -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        let page = page.max(1);
        let mut results = match provider {
            Provider::Tmdb => {
                self.call(provider, Operation::Search, async {
                    SearchProvider::search_multi(&self.tmdb, query, Some(page))
                        .await
                        .map_err(classify)
                })
                .await?
            }
            Provider::AniList => {
                self.call(provider, Operation::Search, async {
                    SearchProvider::search_multi(&self.anilist, query, Some(page))
                        .await
                        .map_err(classify)
                })
                .await?
            }
        };
        // People (and any result kind cameo adds later) are not trackable
        // media, so they are neither items nor unavailable titles.
        results.results.retain(|r| {
            matches!(
                r,
                UnifiedSearchResult::Movie(_) | UnifiedSearchResult::TvShow(_)
            )
        });
        Ok(mapping::page(
            results,
            |r| match r {
                UnifiedSearchResult::Movie(m) => mapping::movie_base(m),
                UnifiedSearchResult::TvShow(t) => mapping::tv_base(t),
                _ => Err(super::id::MediaIdError::Malformed(search_ref(r))),
            },
            search_ref,
        ))
    }

    async fn details_impl(&self, id: &MediaId) -> Result<TitleDetails, MetadataError> {
        let cid = cameo_id(id);
        let mut title = match (id.provider, id.kind) {
            (Provider::Tmdb, MediaKind::Movie) => {
                let d = self
                    .call(id.provider, Operation::Details, async {
                        DetailProvider::movie_details(&self.tmdb, &cid)
                            .await
                            .map_err(classify)
                            .and_then(|d| titled(d, |d| &d.movie.title))
                    })
                    .await?;
                map_title(id, mapping::movie_title(&d))?
            }
            (Provider::Tmdb, MediaKind::Show) => {
                let d = self
                    .call(id.provider, Operation::Details, async {
                        DetailProvider::tv_show_details(&self.tmdb, &cid)
                            .await
                            .map_err(classify)
                            .and_then(|d| titled(d, |d| &d.show.name))
                    })
                    .await?;
                map_title(id, mapping::show_title(&d))?
            }
            (Provider::AniList, MediaKind::Movie) => {
                let d = self
                    .call(id.provider, Operation::Details, async {
                        DetailProvider::movie_details(&self.anilist, &cid)
                            .await
                            .map_err(classify)
                    })
                    .await?;
                map_title(id, mapping::movie_title(&d))?
            }
            (Provider::AniList, MediaKind::Show) => {
                let d = self
                    .call(id.provider, Operation::Details, async {
                        DetailProvider::tv_show_details(&self.anilist, &cid)
                            .await
                            .map_err(classify)
                    })
                    .await?;
                map_title(id, mapping::show_title(&d))?
            }
        };
        if id.provider == Provider::AniList {
            // Best effort: the title is complete without the extras, and a
            // failure here is already counted and logged by `call`.
            let extras = self
                .call_paced(Operation::Extras, self.anilist_extras(id.numeric))
                .await;
            if let Ok(extras) = &extras {
                mapping::apply_anilist_extras(&mut title.snapshot, extras);
            }
            if id.kind == MediaKind::Show {
                // AniList lists no seasons: a show has one, synthesised.
                let next = extras.ok().and_then(|e| e.next_airing_episode);
                let count = anilist::episode_count(title.snapshot.episode_count, next);
                title.seasons = vec![anilist::season_summary(count)];
            }
        }
        Ok(title)
    }

    async fn season_impl(&self, id: &MediaId, season: u32) -> Result<SeasonDetails, MetadataError> {
        if id.kind != MediaKind::Show {
            // A movie has no seasons. Nothing is sent anywhere.
            return Err(unsupported(id.provider, Operation::Season));
        }
        if id.provider == Provider::AniList {
            return self.anilist_season(id, season).await;
        }
        let cid = cameo_id(id);
        let s = self
            .call(id.provider, Operation::Season, async {
                SeasonProvider::season_details(&self.tmdb, &cid, season)
                    .await
                    .map_err(classify)
            })
            .await?;
        Ok(mapping::season_details(&s))
    }

    async fn recommendations_impl(
        &self,
        id: &MediaId,
        page: u32,
    ) -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        let cid = cameo_id(id);
        let page = Some(page.max(1));
        let client: &dyn RecommendationProvider = match id.provider {
            Provider::Tmdb => &self.tmdb,
            // AniList's community recommendations of the title.
            Provider::AniList => &self.anilist,
        };
        Ok(match id.kind {
            MediaKind::Movie => mapping::movies_page(
                self.call(id.provider, Operation::Recommendations, async {
                    client
                        .movie_recommendations(&cid, page)
                        .await
                        .map_err(classify)
                })
                .await?,
            ),
            MediaKind::Show => mapping::shows_page(
                self.call(id.provider, Operation::Recommendations, async {
                    client
                        .tv_recommendations(&cid, page)
                        .await
                        .map_err(classify)
                })
                .await?,
            ),
        })
    }

    async fn feed_impl(
        &self,
        provider: Provider,
        feed: DiscoverCategory,
        kind: MediaKind,
        page: u32,
    ) -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        let op = Operation::Feed;
        match (feed, provider) {
            (DiscoverCategory::Airing, Provider::AniList) => {
                return self.airing_feed(kind, page).await;
            }
            (DiscoverCategory::RegionalDrama(region), Provider::Tmdb)
                if kind == MediaKind::Show =>
            {
                return self.drama_feed(region, page).await;
            }
            _ => {}
        }
        // TMDB trending is one page: cameo's trending call takes no page and
        // answers the first with no next page (justin13888/cameo#194),
        // so a later page asked for is empty rather than the first again.
        let one_page = matches!(feed, DiscoverCategory::Trending) && provider == Provider::Tmdb;
        if one_page && page > 1 {
            return Ok(MetadataPage {
                items: Vec::new(),
                page,
                has_next: false,
                unavailable: Vec::new(),
            });
        }
        let page = Some(page.max(1));
        let client: &dyn DiscoveryProvider = match provider {
            Provider::Tmdb => &self.tmdb,
            Provider::AniList => &self.anilist,
        };
        Ok(match (feed, kind) {
            (DiscoverCategory::Trending, MediaKind::Movie) => mapping::movies_page(
                self.call(provider, op, async {
                    client
                        .trending_movies(TimeWindow::Week, page)
                        .await
                        .map_err(classify)
                })
                .await?,
            ),
            (DiscoverCategory::Trending, MediaKind::Show) => mapping::shows_page(
                self.call(provider, op, async {
                    client
                        .trending_tv_shows(TimeWindow::Week, page)
                        .await
                        .map_err(classify)
                })
                .await?,
            ),
            (DiscoverCategory::Popular, MediaKind::Movie) => mapping::movies_page(
                self.call(provider, op, async {
                    client.popular_movies(page).await.map_err(classify)
                })
                .await?,
            ),
            (DiscoverCategory::Popular, MediaKind::Show) => mapping::shows_page(
                self.call(provider, op, async {
                    client.popular_tv_shows(page).await.map_err(classify)
                })
                .await?,
            ),
            (DiscoverCategory::TopRated, MediaKind::Movie) => mapping::movies_page(
                self.call(provider, op, async {
                    client.top_rated_movies(page).await.map_err(classify)
                })
                .await?,
            ),
            (DiscoverCategory::TopRated, MediaKind::Show) => mapping::shows_page(
                self.call(provider, op, async {
                    client.top_rated_tv_shows(page).await.map_err(classify)
                })
                .await?,
            ),
            // The supported routes returned above. What is left is airing on
            // TMDB (no status filter here), drama on AniList (all anime), or
            // a drama movie: nothing is sent anywhere.
            (DiscoverCategory::Airing | DiscoverCategory::RegionalDrama(_), _) => {
                return Err(unsupported(provider, op));
            }
        })
    }

    /// AniList anime of one kind currently releasing, most popular first,
    /// through cameo's discover builder (which its `DiscoveryProvider` trait
    /// does not expose). Without a format filter the builder asks for
    /// `kind`'s formats only, as cameo splits them.
    async fn airing_feed(
        &self,
        kind: MediaKind,
        page: u32,
    ) -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        let discover = self
            .anilist
            .discover_anime()
            .status(AnimeStatus::Releasing)
            .sort(AnimeSort::PopularityDesc)
            .page(page.max(1));
        let (provider, op) = (Provider::AniList, Operation::Feed);
        Ok(match kind {
            MediaKind::Movie => mapping::movies_page(
                self.call(provider, op, async {
                    discover
                        .execute_movies()
                        .await
                        .map_err(|e| classify(e.into()))
                })
                .await?,
            ),
            MediaKind::Show => mapping::shows_page(
                self.call(provider, op, async {
                    discover.execute_tv().await.map_err(|e| classify(e.into()))
                })
                .await?,
            ),
        })
    }

    /// TMDB's TV discover filtered to one region's scripted shows, most
    /// popular first, through cameo's discover builder (which its
    /// `DiscoveryProvider` trait does not expose).
    ///
    /// The genre exclusion already keeps non-dramas out upstream; a title
    /// whose derived category still is not the region's drama (a
    /// co-production the category rules assign to another region, say) is
    /// left out, so every item is what the feed names.
    async fn drama_feed(
        &self,
        region: DramaRegion,
        page: u32,
    ) -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        let shows = self
            .call(Provider::Tmdb, Operation::Feed, async {
                self.tmdb
                    .discover_tv()
                    // `|` is TMDB's OR between values.
                    .with_origin_country(region.origin_countries().join("|"))
                    .without_genres(&NON_DRAMA_TMDB_GENRES)
                    .sort_by(TvSortBy::PopularityDesc)
                    .page(page.max(1))
                    .execute()
                    .await
                    .map_err(|e| classify(e.into()))
            })
            .await?;
        let mut page = mapping::shows_page(shows);
        let want = region.category();
        page.items.retain(|item| {
            let keep = item.category == want;
            if !keep {
                tracing::debug!(
                    media_id = %item.cameo_media_id,
                    category = ?item.category,
                    "left a non-drama out of a drama feed"
                );
            }
            keep
        });
        Ok(page)
    }
}

/// The cameo id for a Stacks id. cameo's ids carry no kind; the kind picks
/// the call instead.
fn cameo_id(id: &MediaId) -> cameo::MediaId {
    let numeric = u64::try_from(id.numeric).unwrap_or_default();
    match id.provider {
        Provider::Tmdb => cameo::MediaId::tmdb(numeric),
        Provider::AniList => cameo::MediaId::anilist(numeric),
    }
}

fn unsupported(provider: Provider, operation: Operation) -> MetadataError {
    let err = MetadataError::new(provider, operation, MetadataErrorKind::Unsupported);
    report(&err);
    err
}

/// A detail payload whose id does not map is an unusable response.
fn map_title(
    id: &MediaId,
    mapped: Result<TitleDetails, super::id::MediaIdError>,
) -> Result<TitleDetails, MetadataError> {
    mapped.map_err(|e| {
        let err = MetadataError::new(
            id.provider,
            Operation::Details,
            MetadataErrorKind::BadUpstreamResponse(e.to_string()),
        );
        report(&err);
        err
    })
}

fn search_ref(r: &UnifiedSearchResult) -> String {
    match r {
        UnifiedSearchResult::Movie(m) => m.provider_id.to_string(),
        UnifiedSearchResult::TvShow(t) => t.provider_id.to_string(),
        UnifiedSearchResult::Person(p) => p.provider_id.to_string(),
        _ => "unknown".to_owned(),
    }
}

/// A TMDB details response that names no title is not one.
///
/// cameo reads a `null` or missing title as `""` (and a missing id as the
/// one asked for), so such a response decodes. Stored, it would blank the
/// title's catalog snapshot; it is a bad upstream response instead.
fn titled<T>(details: T, title: impl Fn(&T) -> &String) -> Result<T, MetadataErrorKind> {
    if title(&details).trim().is_empty() {
        Err(MetadataErrorKind::BadUpstreamResponse(
            "the details name no title".to_owned(),
        ))
    } else {
        Ok(details)
    }
}

/// Classify one cameo failure.
pub(crate) fn classify(e: ProviderError) -> MetadataErrorKind {
    match e {
        ProviderError::NotFound => MetadataErrorKind::NotFound,
        ProviderError::Unsupported => MetadataErrorKind::Unsupported,
        ProviderError::RateLimited { retry_after } => {
            MetadataErrorKind::RateLimited { retry_after }
        }
        ProviderError::Auth(msg) => MetadataErrorKind::Misconfigured(sanitize(&msg)),
        ProviderError::Api { status, message } => status_kind(status).unwrap_or_else(|| {
            MetadataErrorKind::BadUpstreamResponse(sanitize(&format!("HTTP {status}: {message}")))
        }),
        ProviderError::Transport(msg) => {
            MetadataErrorKind::Unavailable(UnavailableCause::Transport(sanitize(&msg)))
        }
        ProviderError::Deserialization(msg) | ProviderError::Other(msg) => {
            MetadataErrorKind::BadUpstreamResponse(sanitize(&msg))
        }
        ProviderError::InvalidInput(msg) => MetadataErrorKind::Misconfigured(sanitize(&msg)),
        other => MetadataErrorKind::BadUpstreamResponse(sanitize(&other.to_string())),
    }
}

/// The class an error status implies, or `None` for a success status.
fn status_kind(status: u16) -> Option<MetadataErrorKind> {
    Some(match status {
        404 => MetadataErrorKind::NotFound,
        429 => MetadataErrorKind::RateLimited { retry_after: None },
        500..=599 => MetadataErrorKind::Unavailable(UnavailableCause::Status(status)),
        400..=499 => MetadataErrorKind::Misconfigured(format!("HTTP {status}")),
        _ => return None,
    })
}

/// Count and log one classified failure.
fn report(err: &MetadataError) {
    metrics::counter!(
        "stacks_metadata_errors_total",
        "provider" => err.provider.as_str(),
        "operation" => err.operation.as_str(),
        "class" => err.class(),
    )
    .increment(1);
    let (provider, operation, class) = (err.provider.as_str(), err.operation.as_str(), err.class());
    match &err.kind {
        MetadataErrorKind::Misconfigured(msg) => {
            tracing::error!(provider, operation, class, message = %msg, "metadata request rejected; check the provider configuration");
        }
        MetadataErrorKind::BadUpstreamResponse(msg) => {
            tracing::warn!(provider, operation, class, body = %msg, "unusable metadata response");
        }
        MetadataErrorKind::Unavailable(_) | MetadataErrorKind::RateLimited { .. } => {
            tracing::warn!(provider, operation, class, error = %err, "metadata provider unavailable");
        }
        MetadataErrorKind::NotFound | MetadataErrorKind::Unsupported => {
            tracing::debug!(provider, operation, class, "metadata call returned no data");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_table() {
        let api = |status| ProviderError::Api {
            status,
            message: "x".into(),
        };
        let cases: Vec<(ProviderError, &str)> = vec![
            (ProviderError::NotFound, "not_found"),
            (
                ProviderError::RateLimited {
                    retry_after: Some(Duration::from_secs(3)),
                },
                "rate_limited",
            ),
            (ProviderError::Transport("reset".into()), "unavailable"),
            (api(500), "unavailable"),
            (api(503), "unavailable"),
            (api(400), "misconfigured"),
            (api(422), "misconfigured"),
            (ProviderError::Auth("bad key".into()), "misconfigured"),
            (
                ProviderError::Deserialization("eof".into()),
                "bad_upstream_response",
            ),
            (
                ProviderError::Other("graphql".into()),
                "bad_upstream_response",
            ),
            (ProviderError::Unsupported, "unsupported"),
            (ProviderError::InvalidInput("id".into()), "misconfigured"),
        ];
        for (err, class) in cases {
            let shown = err.to_string();
            assert_eq!(classify(err).class(), class, "{shown}");
        }
    }

    #[test]
    fn classified_messages_carry_no_api_key() {
        let leak = "error sending request for url (http://h/3/movie/1?api_key=k3y)";
        for err in [
            ProviderError::Transport(leak.into()),
            ProviderError::Auth(leak.into()),
            ProviderError::Deserialization(leak.into()),
            ProviderError::Other(leak.into()),
            ProviderError::InvalidInput(leak.into()),
            ProviderError::Api {
                status: 418,
                message: leak.into(),
            },
        ] {
            let shown = classify(err).to_string();
            assert!(!shown.contains("k3y"), "{shown}");
        }
    }

    #[test]
    fn credential_shape_picks_the_tmdb_auth_scheme() {
        // Constructing never makes a request, so this only checks it builds.
        let s = ProviderSettings::default();
        assert!(CameoProvider::new("0123456789abcdef0123456789abcdef", &s).is_ok());
        assert!(CameoProvider::new("eyJhbGciOiJIUzI1NiJ9.e30.sig", &s).is_ok());
        assert!(CameoProvider::new("", &s).is_err());
    }

    #[test]
    fn configured_base_urls_reach_the_provider_settings() {
        let config = crate::config::models::CameoConfig {
            tmdb_api_key: "k".to_owned(),
            connect_timeout_secs: 0,
            request_timeout_secs: 7,
            tmdb_base_url: Some("http://fixtures:8080".to_owned()),
            anilist_base_url: Some("http://fixtures:8080/graphql".to_owned()),
        };
        let s = ProviderSettings::from_config(&config);
        assert_eq!(s.tmdb_base_url.as_deref(), Some("http://fixtures:8080"));
        assert_eq!(
            s.anilist_base_url.as_deref(),
            Some("http://fixtures:8080/graphql")
        );
        // A zero timeout would fail every call, so it is raised to a second.
        assert_eq!(s.connect_timeout, Duration::from_secs(1));
        assert_eq!(s.request_timeout, Duration::from_secs(7));
        assert!(CameoProvider::new("0123456789abcdef0123456789abcdef", &s).is_ok());
    }
}
