use std::fmt;
use std::time::Duration;

use thiserror::Error;

use super::id::{MediaIdError, Provider};

#[derive(Debug, Error)]
pub enum MediaError {
    /// The title does not exist upstream, or the id names the wrong kind.
    #[error("media not found: {0}")]
    NotFound(String),
    /// A classified metadata failure other than not-found. The API layer maps
    /// each class onto its own status; none of them is an internal error.
    #[error(transparent)]
    Metadata(MetadataError),
    #[error("invalid media id: {0}")]
    InvalidId(#[from] MediaIdError),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

impl From<MetadataError> for MediaError {
    fn from(e: MetadataError) -> Self {
        match e.kind {
            MetadataErrorKind::NotFound => MediaError::NotFound(e.to_string()),
            _ => MediaError::Metadata(e),
        }
    }
}

/// The metadata operation a call was making, used as a metric and log label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    /// Free-text search.
    Search,
    /// Full detail for one title.
    Details,
    /// One season of a show, with its episodes.
    Season,
    /// Recommendations derived from one title.
    Recommendations,
    /// A discovery feed (trending, popular, top-rated).
    Feed,
    /// Stacks' own supplementary AniList queries: the extras a detail call
    /// adds (romaji title, MAL id), and the lookup of a title by its MAL id.
    Extras,
}

impl Operation {
    /// The lowercase label value.
    pub fn as_str(self) -> &'static str {
        match self {
            Operation::Search => "search",
            Operation::Details => "details",
            Operation::Season => "season",
            Operation::Recommendations => "recommendations",
            Operation::Feed => "feed",
            Operation::Extras => "extras",
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A classified failure of one metadata call.
///
/// Every upstream failure (cameo's `ProviderError`, a timeout, an open
/// circuit, an unusable body) becomes exactly one [`MetadataErrorKind`], so
/// callers decide on the class rather than on provider-specific strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{provider} {operation}: {kind}")]
pub struct MetadataError {
    pub provider: Provider,
    pub operation: Operation,
    pub kind: MetadataErrorKind,
}

impl MetadataError {
    pub fn new(provider: Provider, operation: Operation, kind: MetadataErrorKind) -> Self {
        Self {
            provider,
            operation,
            kind,
        }
    }

    /// The metric/log class label of this failure.
    pub fn class(&self) -> &'static str {
        self.kind.class()
    }

    /// The wait the upstream asked for, when it rate-limited the call.
    pub fn retry_after(&self) -> Option<Duration> {
        match self.kind {
            MetadataErrorKind::RateLimited { retry_after } => retry_after,
            _ => None,
        }
    }
}

/// What went wrong, independent of which provider or operation it was.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MetadataErrorKind {
    /// The provider has no such title (HTTP 404, or a GraphQL not-found).
    #[error("not found")]
    NotFound,
    /// The provider rate-limited the call (HTTP 429).
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    /// The provider could not be reached or answered with a 5xx, the call
    /// ran past its deadline, or the circuit breaker is open.
    #[error("upstream unavailable ({0})")]
    Unavailable(UnavailableCause),
    /// The provider rejected the request itself (a 4xx other than 404/429,
    /// or bad credentials): an operator has to fix the configuration.
    #[error("upstream rejected the request: {0}")]
    Misconfigured(String),
    /// The provider answered, but with a body Stacks cannot use (malformed
    /// or truncated JSON, an explicit `null` in a required field, GraphQL
    /// errors on a 200).
    #[error("unusable upstream response: {0}")]
    BadUpstreamResponse(String),
    /// The provider does not offer this operation (e.g. AniList recommendations).
    #[error("not supported by this provider")]
    Unsupported,
}

impl MetadataErrorKind {
    /// The metric/log class label.
    pub fn class(&self) -> &'static str {
        match self {
            MetadataErrorKind::NotFound => "not_found",
            MetadataErrorKind::RateLimited { .. } => "rate_limited",
            MetadataErrorKind::Unavailable(_) => "unavailable",
            MetadataErrorKind::Misconfigured(_) => "misconfigured",
            MetadataErrorKind::BadUpstreamResponse(_) => "bad_upstream_response",
            MetadataErrorKind::Unsupported => "unsupported",
        }
    }

    /// Whether this failure says the provider itself is unhealthy, and so
    /// counts towards opening its circuit breaker. An open circuit is the
    /// breaker's own verdict and does not count again.
    pub fn trips_breaker(&self) -> bool {
        matches!(
            self,
            MetadataErrorKind::Unavailable(
                UnavailableCause::Transport(_)
                    | UnavailableCause::Status(_)
                    | UnavailableCause::Timeout
            )
        )
    }
}

/// Why a provider counts as unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnavailableCause {
    /// Connection, DNS or TLS failure, or a request timeout inside cameo.
    #[error("transport: {0}")]
    Transport(String),
    /// A 5xx status.
    #[error("HTTP {0}")]
    Status(u16),
    /// The call ran past Stacks' own deadline.
    #[error("deadline exceeded")]
    Timeout,
    /// The provider's circuit breaker is open; no request was sent.
    #[error("circuit open")]
    CircuitOpen,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_health_failures_trip_the_breaker() {
        let trips = [
            MetadataErrorKind::Unavailable(UnavailableCause::Transport("reset".into())),
            MetadataErrorKind::Unavailable(UnavailableCause::Status(503)),
            MetadataErrorKind::Unavailable(UnavailableCause::Timeout),
        ];
        assert!(trips.iter().all(MetadataErrorKind::trips_breaker));
        let neutral = [
            MetadataErrorKind::NotFound,
            MetadataErrorKind::RateLimited { retry_after: None },
            MetadataErrorKind::Unavailable(UnavailableCause::CircuitOpen),
            MetadataErrorKind::Misconfigured("401".into()),
            MetadataErrorKind::BadUpstreamResponse("eof".into()),
            MetadataErrorKind::Unsupported,
        ];
        assert!(!neutral.iter().any(MetadataErrorKind::trips_breaker));
    }

    #[test]
    fn not_found_becomes_media_not_found() {
        let e = MetadataError::new(
            Provider::Tmdb,
            Operation::Details,
            MetadataErrorKind::NotFound,
        );
        assert!(matches!(MediaError::from(e), MediaError::NotFound(_)));
        let e = MetadataError::new(
            Provider::AniList,
            Operation::Season,
            MetadataErrorKind::Unsupported,
        );
        assert!(matches!(MediaError::from(e), MediaError::Metadata(_)));
    }
}
