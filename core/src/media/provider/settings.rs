//! What [`CameoProvider`](super::CameoProvider) is tuned by: timeouts, the
//! retry policy, client-side rates and the circuit breaker.

use std::time::Duration;

use crate::media::breaker::BreakerConfig;

/// Retry tuning, applied to both providers through cameo's `RetryPolicy`.
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

/// Everything [`CameoProvider`](super::CameoProvider) is tuned by.
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
