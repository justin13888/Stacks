pub mod anilist;
pub mod breaker;
pub mod catalog;
pub mod category;
pub mod enrichment;
pub mod error;
pub mod filter;
pub mod id;
pub mod mapping;
pub mod models;
mod pacer;
pub mod provider;
pub mod service;

use std::sync::Arc;

pub use error::{MediaError, MetadataError, MetadataErrorKind, Operation, UnavailableCause};
pub use id::{EpisodeId, MediaId, MediaIdError, MediaKind, Provider};
pub use provider::{
    CameoProvider, DiscoverCategory, DramaRegion, MetadataPage, MetadataProvider,
    ProviderConfigError, ProviderSettings, SeasonDetails, TitleDetails,
};
pub use service::MediaService;

/// Build the production [`MetadataProvider`] from application configuration:
/// cameo over TMDB and AniList, with the configured timeouts and Stacks'
/// retry, rate-limit and circuit-breaker defaults.
///
/// cameo's response cache stays off (see the note on the `cameo` dependency
/// in the workspace `Cargo.toml`); the catalog snapshot is the durable copy.
pub fn build_metadata_provider(
    config: &crate::config::Config,
) -> Result<Arc<dyn MetadataProvider>, ProviderConfigError> {
    let settings = ProviderSettings::from_config(&config.cameo);
    Ok(Arc::new(CameoProvider::new(
        &config.cameo.tmdb_api_key,
        &settings,
    )?))
}
