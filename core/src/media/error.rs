use thiserror::Error;

use super::id::MediaIdError;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("media not found: {0}")]
    NotFound(String),
    #[error("cameo error: {0}")]
    CameoError(String),
    #[error("invalid media id: {0}")]
    InvalidId(#[from] MediaIdError),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

impl From<cameo::CameoClientError> for MediaError {
    fn from(e: cameo::CameoClientError) -> Self {
        if is_not_found(&e) {
            MediaError::NotFound(e.to_string())
        } else {
            MediaError::CameoError(e.to_string())
        }
    }
}

impl From<cameo::AniListError> for MediaError {
    fn from(e: cameo::AniListError) -> Self {
        // AniList is a GraphQL API: a missing entry comes back as a null `Media`
        // rather than an HTTP 404, which cameo surfaces as a not-found error.
        let msg = e.to_string();
        if msg.to_ascii_lowercase().contains("not found") {
            MediaError::NotFound(msg)
        } else {
            MediaError::CameoError(msg)
        }
    }
}

/// Whether a cameo error represents an upstream "not found" (HTTP 404), so it
/// can be surfaced to clients as a 404 rather than an opaque 500.
fn is_not_found(e: &cameo::CameoClientError) -> bool {
    matches!(
        e,
        cameo::CameoClientError::Tmdb(cameo::TmdbError::Api { status: 404, .. })
    )
}
