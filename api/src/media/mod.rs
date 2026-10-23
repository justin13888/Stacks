pub mod dto;
pub mod error;
pub mod handlers;

use salvo::prelude::*;
use stacks_core::entity::media;
use stacks_core::media::{MediaError, MediaId, MediaService, catalog};

use crate::app::AppState;

/// Why a public media id could not be turned into a catalog row.
#[derive(Debug)]
pub enum ResolveError {
    /// Malformed, or the provider has no such title.
    NotFound,
    Internal(String),
}

/// The catalog row for a public id, storing a snapshot the first time a
/// title is seen (a placeholder if the provider is unreachable). For writes.
pub async fn ensure_media(state: &AppState, raw: &str) -> Result<media::Model, ResolveError> {
    let id = MediaId::parse(raw).map_err(|_| ResolveError::NotFound)?;
    MediaService::new(state.db.clone(), state.cameo.clone())
        .ensure_in_catalog(&id)
        .await
        .map_err(|e| match e {
            MediaError::NotFound(_) => ResolveError::NotFound,
            e => ResolveError::Internal(e.to_string()),
        })
}

/// The catalog row for a public id if the catalog has it. For reads: a title
/// never stored cannot have library state.
pub async fn find_media(state: &AppState, raw: &str) -> Result<Option<media::Model>, String> {
    let Ok(id) = MediaId::parse(raw) else {
        return Ok(None);
    };
    catalog::find_by_ref(&state.db, &id)
        .await
        .map_err(|e| e.to_string())
}

pub fn router() -> Router {
    Router::new()
        .push(Router::with_path("media/search").get(handlers::search_media))
        .push(Router::with_path("media/{cameo_id}").get(handlers::get_media))
        .push(Router::with_path("media/{cameo_id}/seasons").get(handlers::get_seasons))
        .push(
            Router::with_path("media/{cameo_id}/seasons/{season}/episodes")
                .get(handlers::get_episodes),
        )
        .push(
            Router::with_path("media/{cameo_id}/recommendations")
                .get(handlers::get_recommendations),
        )
}
