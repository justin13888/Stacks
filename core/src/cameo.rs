//! Cameo client construction and shared types.
//!
//! Wraps the upstream `cameo` SDK with an `Arc<CameoClient>` constructed once
//! at startup from `WW_TMDB_API_KEY`. The same client is shared by all
//! services that need to look up canonical media metadata.
//!
//! Note: cameo's optional on-disk SQLite cache feature is intentionally
//! disabled (its `libsqlite3-sys` version conflicts with sea-orm's). All
//! lookups hit upstream until we reconcile that.

use std::sync::Arc;

use cameo::CameoClient;
use cameo::providers::tmdb::TmdbConfig;
use thiserror::Error;

use crate::config::Config;

#[derive(Debug, Error)]
pub enum CameoInitError {
    #[error("failed to build cameo client: {0}")]
    Build(String),
}

/// Build a shared `CameoClient` from the application config.
pub fn build(config: &Config) -> Result<Arc<CameoClient>, CameoInitError> {
    let client = CameoClient::builder()
        .with_tmdb(TmdbConfig::new(&config.cameo.tmdb_api_key).with_language("en-US"))
        .build()
        .map_err(|e| CameoInitError::Build(e.to_string()))?;

    Ok(Arc::new(client))
}

/// Convenience: render a `cameo:provider_id` into a Stacks-canonical
/// `cameo_media_id` string. Today this is a passthrough — the formats line up
/// — but isolating it here lets us evolve the format if needed.
pub fn id_from_provider(provider_id: &str) -> String {
    provider_id.to_string()
}

/// Parse a Stacks `cameo_media_id` into the numeric TMDB id, when possible.
/// Returns `None` if the id is not a TMDB-shaped reference like `tmdb:550`.
pub fn tmdb_numeric_id(cameo_id: &str) -> Option<i32> {
    cameo_id
        .strip_prefix("tmdb:")
        .and_then(|rest| rest.parse::<i32>().ok())
}
