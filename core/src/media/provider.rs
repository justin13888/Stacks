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

use std::future::Future;
use std::time::Duration;

use cameo::providers::tmdb::builders::TvSortBy;
use cameo::{
    AniListClient, AniListConfig, DetailProvider, DiscoveryProvider, ProviderError, RateLimit,
    RecommendationProvider, RetryPolicy, SearchProvider, SeasonProvider, TimeWindow, TmdbClient,
    TmdbConfig, UnifiedSearchResult,
};
use futures::future::BoxFuture;
use thiserror::Error;
use tokio::time::Instant;

use crate::entity::enums::Category;

use super::anilist;
use super::breaker::{BreakerConfig, BreakerState, CallOutcome, CircuitBreaker};
use super::catalog::MediaSnapshot;
use super::error::{MetadataError, MetadataErrorKind, Operation, UnavailableCause};
use super::id::{MediaId, MediaKind, Provider};
use super::mapping;
use super::models::{EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};
use super::pacer::Pacer;

mod graphql;

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

/// Titles per page of Stacks' own AniList list query (cameo's default).
const ANILIST_PER_PAGE: u32 = 20;

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

/// One season with its episodes.
#[derive(Debug)]
pub struct SeasonDetails {
    pub season: SeasonInfo,
    pub episodes: Vec<EpisodeInfo>,
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

/// Retry tuning, applied to both providers through cameo's [`RetryPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrySettings {
    /// Retries after the first attempt (0 disables retrying).
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

/// A client-side request rate: at most `max_requests` per `per`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateSettings {
    pub max_requests: u32,
    pub per: Duration,
}

/// Everything [`CameoProvider`] is tuned by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSettings {
    /// TCP + TLS connect timeout, per attempt.
    pub connect_timeout: Duration,
    /// Whole-request timeout (connect through body), per attempt.
    pub request_timeout: Duration,
    /// Deadline over one call including every retry and cameo's rate-limit
    /// wait. Stacks' own AniList queries wait for their `anilist_extras_rate`
    /// slot outside it, and refuse a slot further away than it.
    pub call_deadline: Duration,
    pub retry: RetrySettings,
    pub tmdb_rate: RateSettings,
    pub anilist_rate: RateSettings,
    /// Stacks' own AniList queries (extras, airing, MAL lookups), which
    /// cameo's limiter does not see.
    pub anilist_extras_rate: RateSettings,
    pub breaker: BreakerConfig,
    /// TMDB base-URL override (tests point it at a mock server).
    pub tmdb_base_url: Option<String>,
    /// AniList GraphQL endpoint override.
    pub anilist_base_url: Option<String>,
    /// TMDB response language.
    pub tmdb_language: Option<String>,
}

/// The default AniList GraphQL endpoint.
pub const ANILIST_URL: &str = "https://graphql.anilist.co";

impl Default for ProviderSettings {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(3),
            request_timeout: Duration::from_secs(10),
            call_deadline: Duration::from_secs(25),
            retry: RetrySettings {
                max_retries: 2,
                base_delay: Duration::from_millis(250),
                max_delay: Duration::from_secs(2),
            },
            // TMDB documents about 40 requests per 10 seconds.
            tmdb_rate: RateSettings {
                max_requests: 40,
                per: Duration::from_secs(10),
            },
            // AniList allows 90 per minute. cameo gets 60 of them; Stacks' own
            // queries, which cameo's limiter cannot see, get the other 30.
            anilist_rate: RateSettings {
                max_requests: 60,
                per: Duration::from_secs(60),
            },
            // The rest of AniList's 90 a minute.
            anilist_extras_rate: RateSettings {
                max_requests: 30,
                per: Duration::from_secs(60),
            },
            breaker: BreakerConfig::default(),
            tmdb_base_url: None,
            anilist_base_url: None,
            tmdb_language: Some("en-US".to_owned()),
        }
    }
}

impl ProviderSettings {
    /// Defaults with the timeouts from configuration.
    pub fn from_config(config: &crate::config::models::CameoConfig) -> Self {
        Self {
            connect_timeout: Duration::from_secs(config.connect_timeout_secs.max(1)),
            request_timeout: Duration::from_secs(config.request_timeout_secs.max(1)),
            tmdb_base_url: config.tmdb_base_url.clone(),
            anilist_base_url: config.anilist_base_url.clone(),
            ..Self::default()
        }
    }
}

/// The adapter could not be built from its configuration.
#[derive(Debug, Error)]
#[error("metadata provider configuration: {0}")]
pub struct ProviderConfigError(String);

/// [`MetadataProvider`] over cameo's TMDB and AniList clients.
pub struct CameoProvider {
    tmdb: TmdbClient,
    anilist: AniListClient,
    /// Stacks' own AniList requests (the extras, airing and MAL queries).
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
            tmdb_config = tmdb_config
                .with_base_url(url.clone())
                .with_image_base_url(format!("{}/images/", url.trim_end_matches('/')));
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

    /// The breaker state of one provider.
    pub fn circuit_state(&self, provider: Provider) -> BreakerState {
        self.breaker(provider).state()
    }

    fn breaker(&self, provider: Provider) -> &CircuitBreaker {
        match provider {
            Provider::Tmdb => &self.tmdb_breaker,
            Provider::AniList => &self.anilist_breaker,
        }
    }

    /// Run one upstream call under the breaker and the deadline, then
    /// classify, count, time and log its outcome.
    async fn call<T, Fut>(
        &self,
        provider: Provider,
        operation: Operation,
        fut: Fut,
    ) -> Result<T, MetadataError>
    where
        Fut: Future<Output = Result<T, MetadataErrorKind>>,
    {
        self.call_with(provider, operation, None, fut).await
    }

    /// [`call`](Self::call) for one of Stacks' own AniList queries, paced by
    /// `extras_rate`.
    async fn call_paced<T, Fut>(&self, operation: Operation, fut: Fut) -> Result<T, MetadataError>
    where
        Fut: Future<Output = Result<T, MetadataErrorKind>>,
    {
        self.call_with(Provider::AniList, operation, Some(&self.extras_rate), fut)
            .await
    }

    /// The body of [`call`](Self::call). With a `pacer`, the call first waits
    /// for one of its slots, before the breaker and outside the deadline: the
    /// wait is Stacks' own queue, not the provider being slow, so it must
    /// never end in a `Timeout` that trips the breaker. A slot more than the
    /// deadline away is refused at once as `RateLimited`, which leaves the
    /// breaker alone, and a slot the breaker then refuses is given back.
    async fn call_with<T, Fut>(
        &self,
        provider: Provider,
        operation: Operation,
        pacer: Option<&Pacer>,
        fut: Fut,
    ) -> Result<T, MetadataError>
    where
        Fut: Future<Output = Result<T, MetadataErrorKind>>,
    {
        let breaker = self.breaker(provider);
        let start = Instant::now();
        let result = 'call: {
            if pacer.is_some() && breaker.state() == BreakerState::Open {
                // Waiting for a slot only to be refused would be wasted.
                break 'call Err(MetadataErrorKind::Unavailable(
                    UnavailableCause::CircuitOpen,
                ));
            }
            let slot = match pacer {
                Some(pacer) => match pacer.acquire(self.call_deadline).await {
                    Ok(slot) => Some(slot),
                    Err(wait) => {
                        break 'call Err(MetadataErrorKind::RateLimited {
                            retry_after: Some(wait),
                        });
                    }
                },
                None => None,
            };
            if !breaker.try_acquire() {
                // Dropping the slot gives it back: nothing is sent.
                drop(slot);
                break 'call Err(MetadataErrorKind::Unavailable(
                    UnavailableCause::CircuitOpen,
                ));
            }
            if let Some(slot) = slot {
                slot.used();
            }
            let r = match tokio::time::timeout(self.call_deadline, fut).await {
                Ok(r) => r,
                Err(_) => Err(MetadataErrorKind::Unavailable(UnavailableCause::Timeout)),
            };
            breaker.record(match &r {
                Err(kind) if kind.trips_breaker() => CallOutcome::Unhealthy,
                _ => CallOutcome::Healthy,
            });
            r
        };
        let outcome = match &result {
            Ok(_) => "ok",
            Err(kind) => kind.class(),
        };
        metrics::histogram!(
            "stacks_metadata_request_duration_seconds",
            "provider" => provider.as_str(),
            "operation" => operation.as_str(),
            "outcome" => outcome,
        )
        .record(start.elapsed().as_secs_f64());
        result.map_err(|kind| {
            let err = MetadataError::new(provider, operation, kind);
            report(&err);
            err
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
        Box::pin(self.call_paced(Operation::Extras, self.anilist_mal_lookup(mal_id)))
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
        if id.provider != Provider::Tmdb {
            // cameo has no AniList recommendations.
            return Err(unsupported(id.provider, Operation::Recommendations));
        }
        let cid = cameo_id(id);
        let page = Some(page.max(1));
        Ok(match id.kind {
            MediaKind::Movie => mapping::movies_page(
                self.call(id.provider, Operation::Recommendations, async {
                    RecommendationProvider::movie_recommendations(&self.tmdb, &cid, page)
                        .await
                        .map_err(classify)
                })
                .await?,
            ),
            MediaKind::Show => mapping::shows_page(
                self.call(id.provider, Operation::Recommendations, async {
                    RecommendationProvider::tv_recommendations(&self.tmdb, &cid, page)
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
                let p = self
                    .call_paced(op, self.anilist_airing(kind, page.max(1)))
                    .await?;
                return Ok(mapping::anilist_list_page(p, kind, page.max(1)));
            }
            (DiscoverCategory::RegionalDrama(region), Provider::Tmdb)
                if kind == MediaKind::Show =>
            {
                return self.drama_feed(region, page).await;
            }
            _ => {}
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

    /// TMDB's TV discover filtered to one region's scripted shows, most
    /// popular first. Through cameo's discover builder, which its
    /// `DiscoveryProvider` trait does not expose (Stacks#14).
    ///
    /// The genre exclusion already keeps non-dramas out upstream; a title
    /// whose derived category still is not the region's drama (a
    /// co-production the category rules assign to another region, say) is
    /// left out, so every item is what the feed names. A single attempt:
    /// cameo's discover builder does not run its retry policy.
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

/// Classify one cameo failure.
pub(crate) fn classify(e: ProviderError) -> MetadataErrorKind {
    match e {
        ProviderError::NotFound => MetadataErrorKind::NotFound,
        ProviderError::Unsupported => MetadataErrorKind::Unsupported,
        ProviderError::RateLimited { retry_after } => {
            MetadataErrorKind::RateLimited { retry_after }
        }
        ProviderError::Auth(msg) => MetadataErrorKind::Misconfigured(truncate(&msg)),
        ProviderError::Api { status, message } => status_kind(status).unwrap_or_else(|| {
            MetadataErrorKind::BadUpstreamResponse(truncate(&format!("HTTP {status}: {message}")))
        }),
        ProviderError::Transport(msg) => {
            MetadataErrorKind::Unavailable(UnavailableCause::Transport(truncate(&msg)))
        }
        ProviderError::Deserialization(msg) | ProviderError::Other(msg) => {
            MetadataErrorKind::BadUpstreamResponse(truncate(&msg))
        }
        ProviderError::InvalidInput(msg) => MetadataErrorKind::Misconfigured(truncate(&msg)),
        other => MetadataErrorKind::BadUpstreamResponse(truncate(&other.to_string())),
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

fn transport_kind(e: reqwest::Error) -> MetadataErrorKind {
    if e.is_timeout() {
        MetadataErrorKind::Unavailable(UnavailableCause::Timeout)
    } else if e.is_decode() {
        MetadataErrorKind::BadUpstreamResponse(truncate(&e.to_string()))
    } else {
        MetadataErrorKind::Unavailable(UnavailableCause::Transport(truncate(&e.to_string())))
    }
}

/// Bound an upstream message (which may carry a response body) for logs.
fn truncate(s: &str) -> String {
    const MAX: usize = 512;
    if s.len() <= MAX {
        return s.to_owned();
    }
    let mut end = MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
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
    fn truncation_respects_char_boundaries() {
        let s = "é".repeat(400);
        let t = truncate(&s);
        assert!(t.len() <= 512 + '…'.len_utf8());
        assert!(t.ends_with('…'));
        assert_eq!(truncate("short"), "short");
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
