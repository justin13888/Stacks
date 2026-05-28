//! Canonical, self-describing media identifiers.
//!
//! cameo's `provider_id` (e.g. `"tmdb:550"`) names a provider and a numeric id
//! but does not record whether the id is a movie or a TV show — yet cameo's
//! detail / season / recommendation / watch-provider calls need that
//! distinction (they expose separate `movie_*` and `tv_*` methods). Stacks
//! therefore persists a richer, self-describing id in every `cameo_media_id`
//! column:
//!
//! ```text
//! <provider>:<kind>:<numeric>     e.g. tmdb:movie:550, tmdb:tv:1396, anilist:tv:21
//! ```
//!
//! Episode ids extend this with season and episode numbers:
//!
//! ```text
//! <provider>:tv:<show>:<season>:<episode>     e.g. tmdb:tv:1396:1:1
//! ```
//!
//! The kind is always known where a cameo result is consumed (a `UnifiedMovie`
//! is a movie, a `UnifiedTvShow` is a show), so building an id is unambiguous;
//! parsing a stored id recovers the kind needed to route back into cameo.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Failure parsing a [`MediaId`] or [`EpisodeId`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaIdError {
    /// The id did not have the expected shape.
    #[error("malformed media id: {0:?}")]
    Malformed(String),
    /// The provider segment was not recognised.
    #[error("unknown media provider: {0:?}")]
    UnknownProvider(String),
    /// The kind segment was not recognised.
    #[error("unknown media kind: {0:?}")]
    UnknownKind(String),
}

/// Metadata provider behind an id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    /// The Movie Database.
    Tmdb,
    /// AniList (anime / manga).
    AniList,
}

impl Provider {
    /// The lowercase wire/storage token for this provider.
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Tmdb => "tmdb",
            Provider::AniList => "anilist",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Provider {
    type Err = MediaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tmdb" => Ok(Provider::Tmdb),
            "anilist" => Ok(Provider::AniList),
            other => Err(MediaIdError::UnknownProvider(other.to_owned())),
        }
    }
}

/// Whether an id names a movie or a TV show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    /// A movie.
    Movie,
    /// A TV show.
    Tv,
}

impl MediaKind {
    /// The lowercase wire/storage token for this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Movie => "movie",
            MediaKind::Tv => "tv",
        }
    }
}

impl fmt::Display for MediaKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MediaKind {
    type Err = MediaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "movie" => Ok(MediaKind::Movie),
            "tv" => Ok(MediaKind::Tv),
            other => Err(MediaIdError::UnknownKind(other.to_owned())),
        }
    }
}

/// A canonical, self-describing Stacks media identifier.
///
/// This is the value stored in every `cameo_media_id` column.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaId {
    /// The metadata provider.
    pub provider: Provider,
    /// Whether this is a movie or a TV show.
    pub kind: MediaKind,
    /// The provider-native numeric id.
    pub numeric: i32,
}

impl MediaId {
    /// Construct a [`MediaId`] from its parts.
    pub fn new(provider: Provider, kind: MediaKind, numeric: i32) -> Self {
        Self {
            provider,
            kind,
            numeric,
        }
    }

    /// Parse a stored id such as `"tmdb:movie:550"`.
    pub fn parse(s: &str) -> Result<Self, MediaIdError> {
        let malformed = || MediaIdError::Malformed(s.to_owned());
        let mut segments = s.split(':');
        let provider = segments.next().ok_or_else(malformed)?;
        let kind = segments.next().ok_or_else(malformed)?;
        let numeric = segments.next().ok_or_else(malformed)?;
        if segments.next().is_some() {
            return Err(malformed());
        }
        Ok(Self {
            provider: provider.parse()?,
            kind: kind.parse()?,
            numeric: numeric.parse().map_err(|_| malformed())?,
        })
    }

    /// Build a [`MediaId`] from a cameo `provider_id` (`"tmdb:550"`) plus the
    /// [`MediaKind`] known at the call site.
    pub fn from_cameo(provider_id: &str, kind: MediaKind) -> Result<Self, MediaIdError> {
        let malformed = || MediaIdError::Malformed(provider_id.to_owned());
        let mut segments = provider_id.split(':');
        let provider = segments.next().ok_or_else(malformed)?;
        let numeric = segments.next().ok_or_else(malformed)?;
        if segments.next().is_some() {
            return Err(malformed());
        }
        Ok(Self {
            provider: provider.parse()?,
            kind,
            numeric: numeric.parse().map_err(|_| malformed())?,
        })
    }

    /// The string form persisted in the database and returned to clients.
    pub fn to_storage_string(&self) -> String {
        format!("{}:{}:{}", self.provider, self.kind, self.numeric)
    }

    /// The cameo `provider_id` form (`"<provider>:<numeric>"`), used when
    /// calling cameo's cache-lookup / watch-provider APIs.
    pub fn to_cameo_provider_id(&self) -> String {
        format!("{}:{}", self.provider, self.numeric)
    }
}

impl fmt::Display for MediaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.provider, self.kind, self.numeric)
    }
}

impl FromStr for MediaId {
    type Err = MediaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// A canonical, self-describing episode identifier.
///
/// Stored in `episode_watch_status.cameo_episode_id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EpisodeId {
    /// The metadata provider.
    pub provider: Provider,
    /// The provider-native numeric id of the parent show.
    pub show: i32,
    /// The season number (0 is the specials season).
    pub season: u32,
    /// The episode number within the season.
    pub episode: u32,
}

impl EpisodeId {
    /// Construct an [`EpisodeId`] from its parts.
    pub fn new(provider: Provider, show: i32, season: u32, episode: u32) -> Self {
        Self {
            provider,
            show,
            season,
            episode,
        }
    }

    /// Parse a stored id such as `"tmdb:tv:1396:1:1"`.
    pub fn parse(s: &str) -> Result<Self, MediaIdError> {
        let malformed = || MediaIdError::Malformed(s.to_owned());
        let mut segments = s.split(':');
        let provider = segments.next().ok_or_else(malformed)?;
        let kind = segments.next().ok_or_else(malformed)?;
        let show = segments.next().ok_or_else(malformed)?;
        let season = segments.next().ok_or_else(malformed)?;
        let episode = segments.next().ok_or_else(malformed)?;
        if segments.next().is_some() || kind != MediaKind::Tv.as_str() {
            return Err(malformed());
        }
        Ok(Self {
            provider: provider.parse()?,
            show: show.parse().map_err(|_| malformed())?,
            season: season.parse().map_err(|_| malformed())?,
            episode: episode.parse().map_err(|_| malformed())?,
        })
    }

    /// The string form persisted in the database.
    pub fn to_storage_string(&self) -> String {
        format!(
            "{}:tv:{}:{}:{}",
            self.provider, self.show, self.season, self.episode
        )
    }

    /// The [`MediaId`] of the show this episode belongs to.
    pub fn show_id(&self) -> MediaId {
        MediaId::new(self.provider, MediaKind::Tv, self.show)
    }
}

impl fmt::Display for EpisodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_storage_string())
    }
}

impl FromStr for EpisodeId {
    type Err = MediaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_id_round_trips() {
        for raw in [
            "tmdb:movie:550",
            "tmdb:tv:1396",
            "anilist:tv:21",
            "anilist:movie:199",
        ] {
            let id = MediaId::parse(raw).expect("parse");
            assert_eq!(id.to_storage_string(), raw);
            assert_eq!(raw.parse::<MediaId>().unwrap(), id);
        }
    }

    #[test]
    fn media_id_exposes_parts() {
        let id = MediaId::parse("tmdb:movie:550").unwrap();
        assert_eq!(id.provider, Provider::Tmdb);
        assert_eq!(id.kind, MediaKind::Movie);
        assert_eq!(id.numeric, 550);
    }

    #[test]
    fn media_id_from_cameo_provider_id() {
        let id = MediaId::from_cameo("tmdb:1396", MediaKind::Tv).unwrap();
        assert_eq!(id, MediaId::new(Provider::Tmdb, MediaKind::Tv, 1396));
        assert_eq!(id.to_cameo_provider_id(), "tmdb:1396");
    }

    #[test]
    fn media_id_rejects_malformed() {
        assert!(MediaId::parse("tmdb:550").is_err()); // missing kind
        assert!(MediaId::parse("tmdb:movie:550:extra").is_err()); // too many parts
        assert!(MediaId::parse("tmdb:movie:abc").is_err()); // non-numeric
        assert!(MediaId::parse("netflix:movie:1").is_err()); // unknown provider
        assert!(MediaId::parse("tmdb:song:1").is_err()); // unknown kind
        assert!(MediaId::parse("").is_err());
    }

    #[test]
    fn episode_id_round_trips() {
        let raw = "tmdb:tv:1396:1:1";
        let id = EpisodeId::parse(raw).expect("parse");
        assert_eq!(id.to_storage_string(), raw);
        assert_eq!(id.show, 1396);
        assert_eq!(id.season, 1);
        assert_eq!(id.episode, 1);
        assert_eq!(
            id.show_id(),
            MediaId::new(Provider::Tmdb, MediaKind::Tv, 1396)
        );
    }

    #[test]
    fn episode_id_rejects_bad_input() {
        assert!(EpisodeId::parse("tmdb:movie:1396:1:1").is_err()); // not tv
        assert!(EpisodeId::parse("tmdb:tv:1396:1").is_err()); // missing episode
    }
}
