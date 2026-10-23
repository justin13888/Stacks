//! The failures many operations share, as RFC 9457 problem types.
//!
//! Every error the API answers with is an `application/problem+json`
//! document. Each operation names its own error type, so its declared
//! responses are exactly the statuses it can produce; the types here are the
//! building blocks those per-operation enums hold with
//! `#[problem(transparent)]`.
//!
//! Every problem `type` is [`BASE`] followed by a slug, and each slug is
//! described in `docs/adr/0006-error-model.md`, under a heading of the same
//! name, which is where [`BASE`] points.

use std::fmt::Display;
use std::time::Duration;

use kynos::prelude::*;
use stacks_core::media::{MediaError, MetadataError, MetadataErrorKind, UnavailableCause};

/// The prefix of every problem type Stacks defines.
pub const BASE: &str =
    "https://github.com/justin13888/Stacks/blob/master/docs/adr/0006-error-model.md#";

/// Something failed that the client could not have caused and cannot fix.
///
/// The cause is logged where it happened and never sent: a problem document
/// is not a debugging channel.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("an internal error occurred")]
#[problem(base = crate::problem::BASE, status = 500, title = "Internal error")]
pub struct Internal;

impl Internal {
    /// Log `cause` and return the opaque 500 that reports it.
    #[track_caller]
    pub fn log(cause: impl Display) -> Self {
        let location = std::panic::Location::caller();
        tracing::error!(error = %cause, at = %location, "internal error");
        Self
    }
}

impl From<sea_orm::DbErr> for Internal {
    #[track_caller]
    fn from(e: sea_orm::DbErr) -> Self {
        Self::log(e)
    }
}

/// The title a request named does not exist, upstream or in the catalog.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("no such title: {0}")]
#[problem(base = crate::problem::BASE, status = 404, title = "Title not found")]
pub struct TitleNotFound(pub String);

/// How long a client should wait before trying again.
#[derive(Debug, HeaderParams)]
pub struct RetryAfter {
    /// Seconds until the metadata provider accepts requests again.
    #[header(rename = "Retry-After")]
    pub retry_after: u64,
}

/// A metadata provider (TMDB, AniList) failed, and no stored snapshot could
/// stand in for it.
#[derive(Debug, thiserror::Error, ApiError)]
#[problem(base = crate::problem::BASE)]
pub enum Upstream {
    /// The provider could not be reached, timed out, answered with a 5xx, or
    /// its circuit breaker is open because it has been failing.
    #[error("the metadata provider is unavailable")]
    #[problem(status = 503, title = "Metadata provider unavailable")]
    UpstreamUnavailable,

    /// The provider is rate-limiting Stacks.
    #[error("the metadata provider is rate-limiting requests")]
    #[problem(status = 503, title = "Metadata provider rate-limited")]
    UpstreamRateLimited {
        #[problem(headers)]
        retry: RetryAfter,
    },

    /// The provider answered with something Stacks cannot use, or refused a
    /// request an operator has to fix (credentials, configuration).
    #[error("the metadata provider returned an unusable response")]
    #[problem(status = 502, title = "Metadata provider error")]
    UpstreamInvalidResponse,
}

/// When the provider gives no `Retry-After`, a short wait is still better
/// than an immediate retry against a provider that just refused.
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(30);

impl Upstream {
    /// The response a classified metadata failure becomes. Not-found is the
    /// caller's to map (it is a 404 of the operation's own), so it is not
    /// handled here.
    pub fn from_metadata(e: &MetadataError) -> Self {
        tracing::warn!(
            provider = %e.provider,
            operation = %e.operation,
            class = e.class(),
            error = %e,
            "metadata provider failed"
        );
        match &e.kind {
            MetadataErrorKind::RateLimited { retry_after } => Self::UpstreamRateLimited {
                retry: RetryAfter {
                    retry_after: retry_after.unwrap_or(DEFAULT_RETRY_AFTER).as_secs().max(1),
                },
            },
            MetadataErrorKind::Unavailable(
                UnavailableCause::Transport(_)
                | UnavailableCause::Status(_)
                | UnavailableCause::Timeout
                | UnavailableCause::CircuitOpen,
            ) => Self::UpstreamUnavailable,
            MetadataErrorKind::Misconfigured(_)
            | MetadataErrorKind::BadUpstreamResponse(_)
            | MetadataErrorKind::NotFound
            | MetadataErrorKind::Unsupported => Self::UpstreamInvalidResponse,
        }
    }
}

/// What a catalog read can fail with: the title is missing, the provider
/// failed with no snapshot to fall back on, or Stacks itself failed.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum CatalogError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] TitleNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Upstream(#[from] Upstream),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<MediaError> for CatalogError {
    fn from(e: MediaError) -> Self {
        match e {
            MediaError::NotFound(what) => TitleNotFound(what).into(),
            MediaError::Metadata(e) if e.kind == MetadataErrorKind::Unsupported => {
                TitleNotFound(e.to_string()).into()
            }
            MediaError::Metadata(e) => Upstream::from_metadata(&e).into(),
            // A ref the path already parsed cannot be invalid here.
            MediaError::InvalidId(e) => Internal::log(e).into(),
            MediaError::Db(e) => Internal::from(e).into(),
        }
    }
}
