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
//!   errors only, honouring `Retry-After`) and a per-provider [`RateLimit`];
//! - a [`CircuitBreaker`] per provider;
//! - routing on [`MediaId::provider`]: an AniList id never reaches TMDB, and
//!   a capability the provider lacks is [`MetadataErrorKind::Unsupported`]
//!   without any request;
//! - one [`MetadataError`] class per failure, counted in
//!   `stacks_metadata_errors_total{provider,operation,class}`, with every call
//!   timed in `stacks_metadata_request_duration_seconds{provider,operation,outcome}`.

use std::future::Future;
use std::time::Duration;

use cameo::{
    AniListClient, AniListConfig, DetailProvider, DiscoveryProvider, ProviderError, RateLimit,
    RecommendationProvider, RetryPolicy, SearchProvider, SeasonProvider, TimeWindow, TmdbClient,
    TmdbConfig, UnifiedSearchResult,
};
use futures::future::BoxFuture;
use serde::Deserialize;
use thiserror::Error;
use tokio::time::Instant;

use super::breaker::{BreakerConfig, BreakerState, CallOutcome, CircuitBreaker};
use super::catalog::MediaSnapshot;
use super::error::{MetadataError, MetadataErrorKind, Operation, UnavailableCause};
use super::id::{MediaId, MediaKind, Provider};
use super::mapping::{self, AniListExtras};
use super::models::{EpisodeInfo, MediaDetails, MediaWithUserState, SeasonInfo};

/// A discovery feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverCategory {
    /// Trending over the past week.
    Trending,
    /// All-time popular.
    Popular,
    /// Highest rated.
    TopRated,
}

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
    /// Season summaries (empty for movies, and for AniList, which has none).
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
    /// Deadline over one call including every retry and rate-limit wait.
    pub call_deadline: Duration,
    pub retry: RetrySettings,
    pub tmdb_rate: RateSettings,
    pub anilist_rate: RateSettings,
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
            // AniList allows 90 per minute. cameo gets 60 of them; the rest is
            // headroom for Stacks' own extras query, which cameo's limiter
            // cannot see.
            anilist_rate: RateSettings {
                max_requests: 60,
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
            ..Self::default()
        }
    }
}

/// The adapter could not be built from its configuration.
#[derive(Debug, Error)]
#[error("metadata provider configuration: {0}")]
pub struct ConfigError(String);

/// [`MetadataProvider`] over cameo's TMDB and AniList clients.
pub struct CameoProvider {
    tmdb: TmdbClient,
    anilist: AniListClient,
    /// Stacks' own AniList requests (the extras query).
    extras_http: reqwest::Client,
    anilist_url: String,
    tmdb_breaker: CircuitBreaker,
    anilist_breaker: CircuitBreaker,
    call_deadline: Duration,
}

impl CameoProvider {
    /// Build the adapter. TMDB needs a credential (a v4 read token, or a v3
    /// key, which is shorter); AniList needs none.
    pub fn new(tmdb_credential: &str, settings: &ProviderSettings) -> Result<Self, ConfigError> {
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
        let tmdb = TmdbClient::new(tmdb_config).map_err(|e| ConfigError(e.to_string()))?;

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
        .map_err(|e| ConfigError(e.to_string()))?;

        let extras_http = reqwest::Client::builder()
            .user_agent(concat!("stacks/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(settings.connect_timeout)
            .timeout(settings.request_timeout)
            .build()
            .map_err(|e| ConfigError(e.to_string()))?;

        Ok(Self {
            tmdb,
            anilist,
            extras_http,
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
        let breaker = self.breaker(provider);
        let start = Instant::now();
        let result = if breaker.try_acquire() {
            let r = match tokio::time::timeout(self.call_deadline, fut).await {
                Ok(r) => r,
                Err(_) => Err(MetadataErrorKind::Unavailable(UnavailableCause::Timeout)),
            };
            breaker.record(match &r {
                Err(kind) if kind.trips_breaker() => CallOutcome::Unhealthy,
                _ => CallOutcome::Healthy,
            });
            r
        } else {
            Err(MetadataErrorKind::Unavailable(
                UnavailableCause::CircuitOpen,
            ))
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

    /// Stacks' own AniList query for what cameo drops: the romaji title,
    /// synonyms, the MAL id and the raw country of origin. Workaround for
    /// cameo gaps tracked in justin13888/Stacks#14.
    async fn anilist_extras(&self, numeric: i32) -> Result<AniListExtras, MetadataErrorKind> {
        const QUERY: &str = "query StacksMediaExtras($id: Int) { Media(id: $id, type: ANIME) { \
            idMal title { romaji english native } synonyms countryOfOrigin format episodes \
            duration status } }";
        #[derive(Deserialize)]
        struct Envelope {
            data: Option<Data>,
            #[serde(default)]
            errors: Vec<GqlError>,
        }
        #[derive(Deserialize)]
        struct Data {
            #[serde(rename = "Media")]
            media: Option<AniListExtras>,
        }
        #[derive(Deserialize)]
        struct GqlError {
            message: Option<String>,
            status: Option<i64>,
        }

        let resp = self
            .extras_http
            .post(&self.anilist_url)
            .json(&serde_json::json!({ "query": QUERY, "variables": { "id": numeric } }))
            .send()
            .await
            .map_err(transport_kind)?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = resp.text().await.map_err(transport_kind)?;
        if status == 429 {
            return Err(MetadataErrorKind::RateLimited { retry_after });
        }
        let envelope: Envelope = match serde_json::from_str(&body) {
            Ok(e) => e,
            Err(e) => {
                return Err(match status_kind(status) {
                    Some(kind) => kind,
                    None => MetadataErrorKind::BadUpstreamResponse(truncate(&format!(
                        "{e}; body: {body}"
                    ))),
                });
            }
        };
        if envelope.errors.iter().any(|e| e.status == Some(404)) {
            return Err(MetadataErrorKind::NotFound);
        }
        if envelope.errors.iter().any(|e| e.status == Some(429)) {
            return Err(MetadataErrorKind::RateLimited { retry_after });
        }
        match envelope.data.and_then(|d| d.media) {
            Some(extras) => Ok(extras),
            None if !envelope.errors.is_empty() => {
                let messages: Vec<&str> = envelope
                    .errors
                    .iter()
                    .filter_map(|e| e.message.as_deref())
                    .collect();
                Err(MetadataErrorKind::BadUpstreamResponse(truncate(&format!(
                    "GraphQL errors: {}",
                    messages.join("; ")
                ))))
            }
            None => Err(status_kind(status).unwrap_or(MetadataErrorKind::NotFound)),
        }
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
            if let Ok(extras) = self
                .call(
                    id.provider,
                    Operation::Extras,
                    self.anilist_extras(id.numeric),
                )
                .await
            {
                mapping::apply_anilist_extras(&mut title.snapshot, &extras);
            }
        }
        Ok(title)
    }

    async fn season_impl(&self, id: &MediaId, season: u32) -> Result<SeasonDetails, MetadataError> {
        if id.provider != Provider::Tmdb || id.kind != MediaKind::Show {
            // AniList models anime as one continuous run and has no seasons;
            // a movie has none either. Nothing is sent anywhere.
            return Err(unsupported(id.provider, Operation::Season));
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
        let page = Some(page.max(1));
        let client: &dyn DiscoveryProvider = match provider {
            Provider::Tmdb => &self.tmdb,
            Provider::AniList => &self.anilist,
        };
        let op = Operation::Feed;
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
        })
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
}
