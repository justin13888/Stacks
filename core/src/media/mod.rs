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

use cameo::{AniListClient, AniListConfig, CameoClient, CameoClientError, TmdbConfig};

/// The metadata clients Stacks talks to.
///
/// cameo's unified facade dispatches TMDB-first and only falls back to AniList
/// when TMDB is absent, so with both providers configured an `anilist:*` id
/// would silently resolve against TMDB and return an unrelated title. Keeping a
/// directly-addressable [`AniListClient`] alongside the facade lets
/// [`service::MediaService`] route on the id's provider.
///
/// Derefs to the [`CameoClient`], so the many call sites that only need the
/// unified facade are unaffected.
pub struct MediaClient {
    cameo: CameoClient,
    anilist: AniListClient,
}

impl MediaClient {
    /// The AniList client, for ids whose provider is [`Provider::AniList`].
    pub fn anilist(&self) -> &AniListClient {
        &self.anilist
    }
}

impl std::ops::Deref for MediaClient {
    type Target = CameoClient;

    fn deref(&self) -> &Self::Target {
        &self.cameo
    }
}

/// Build the shared cameo client from application configuration.
///
/// Both providers are configured. AniList needs no credentials, and the
/// `anilist` cargo feature was already enabled — leaving it unconfigured meant
/// every `anilist:*` id fell through to TMDB and resolved to an unrelated
/// title.
///
/// Response caching is intentionally not enabled — see the note on the `cameo`
/// dependency in the workspace `Cargo.toml`.
pub fn build_cameo_client(config: &crate::config::Config) -> Result<MediaClient, CameoClientError> {
    let cameo = CameoClient::builder()
        .with_tmdb(TmdbConfig::new(config.cameo.tmdb_api_key.as_str()).with_language("en-US"))
        .with_anilist(AniListConfig::new())
        .build()?;
    let anilist =
        AniListClient::new(AniListConfig::new()).map_err(cameo::CameoClientError::AniList)?;
    Ok(MediaClient { cameo, anilist })
}
