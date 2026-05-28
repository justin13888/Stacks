pub mod enrichment;
pub mod error;
pub mod filter;
pub mod id;
pub mod mapping;
pub mod models;
pub mod service;

pub use error::MediaError;
pub use id::{EpisodeId, MediaId, MediaIdError, MediaKind, Provider};
pub use service::{DiscoverCategory, MediaService};

use cameo::{CameoClient, CameoClientError, TmdbConfig};

/// Build the shared cameo client from application configuration.
///
/// Response caching is intentionally not enabled — see the note on the `cameo`
/// dependency in the workspace `Cargo.toml`.
pub fn build_cameo_client(config: &crate::config::Config) -> Result<CameoClient, CameoClientError> {
    CameoClient::builder()
        .with_tmdb(TmdbConfig::new(config.cameo.tmdb_api_key.as_str()).with_language("en-US"))
        .build()
}
