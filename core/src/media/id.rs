//! Canonical, self-describing public media identifiers.
//!
//! Internally every table keys titles by a UUID (`media.id`). The id clients
//! see — and the one a title is looked up by — is the self-describing ref of
//! its primary `media_refs` row:
//!
//! ```text
//! <provider>:<kind>:<numeric>     e.g. tmdb:movie:550, tmdb:show:1396, anilist:show:21
//! ```
//!
//! cameo's `provider_id` (`"tmdb:550"`) names a provider and a numeric id but
//! not whether the id is a movie or a show, yet cameo routes the two through
//! separate calls; the kind is therefore folded in at Stacks's edge.
//!
//! Episode ids extend a show id with season and episode numbers:
//!
//! ```text
//! <provider>:show:<show>:<season>:<episode>     e.g. tmdb:show:1396:1:1
//! ```
//!
//! The kind token `tv` is still accepted when parsing, as an alias of `show`,
//! so ids issued before the baseline schema keep resolving.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

pub use crate::entity::enums::MediaKind;
use crate::entity::enums::RefProvider;

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

/// A metadata provider Stacks can fetch a title from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    /// The Movie Database.
    Tmdb,
    /// AniList (anime).
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

    /// The `media_refs.provider` value for this provider.
    pub fn as_ref_provider(self) -> RefProvider {
        match self {
            Provider::Tmdb => RefProvider::Tmdb,
            Provider::AniList => RefProvider::Anilist,
        }
    }

    /// The fetchable provider behind a stored ref provider, if any.
    pub fn from_ref_provider(p: RefProvider) -> Option<Self> {
        match p {
            RefProvider::Tmdb => Some(Provider::Tmdb),
            RefProvider::Anilist => Some(Provider::AniList),
            RefProvider::Mal | RefProvider::Imdb | RefProvider::Tvdb => None,
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

/// Parse a kind token, accepting the legacy `tv` alias for `show`.
pub fn parse_kind(s: &str) -> Result<MediaKind, MediaIdError> {
    match s {
        "movie" => Ok(MediaKind::Movie),
        "show" | "tv" => Ok(MediaKind::Show),
        other => Err(MediaIdError::UnknownKind(other.to_owned())),
    }
}

/// Parse a numeric id segment, accepting only its canonical decimal form:
/// ASCII digits with no sign and no leading zero, within `T`'s range, so the
/// parsed value prints back as exactly `s`. `"0"` is accepted only when
/// `allow_zero` is set (season and episode numbers; specials are season 0).
fn parse_canonical<T: FromStr>(s: &str, allow_zero: bool) -> Option<T> {
    let digits_only = !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let leading_zero = s.len() > 1 && s.starts_with('0');
    if !digits_only || leading_zero || (s == "0" && !allow_zero) {
        return None;
    }
    s.parse().ok()
}

/// A canonical, self-describing public media id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MediaId {
    /// The metadata provider.
    pub provider: Provider,
    /// Whether this is a movie or a show.
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

    /// Parse a public id such as `"tmdb:movie:550"`.
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
            kind: parse_kind(kind)?,
            numeric: parse_canonical(numeric, false).ok_or_else(malformed)?,
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
            numeric: parse_canonical(numeric, false).ok_or_else(malformed)?,
        })
    }

    /// The public string form.
    pub fn to_storage_string(&self) -> String {
        self.to_string()
    }

    /// The `media_refs.external_id` value.
    pub fn external_id(&self) -> String {
        self.numeric.to_string()
    }

    /// The cameo `provider_id` form (`"<provider>:<numeric>"`).
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

/// A canonical, self-describing episode id.
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

    /// Parse an id such as `"tmdb:show:1396:1:1"` (or the legacy `tmdb:tv:…`).
    pub fn parse(s: &str) -> Result<Self, MediaIdError> {
        let malformed = || MediaIdError::Malformed(s.to_owned());
        let mut segments = s.split(':');
        let provider = segments.next().ok_or_else(malformed)?;
        let kind = segments.next().ok_or_else(malformed)?;
        let show = segments.next().ok_or_else(malformed)?;
        let season = segments.next().ok_or_else(malformed)?;
        let episode = segments.next().ok_or_else(malformed)?;
        if segments.next().is_some() || parse_kind(kind) != Ok(MediaKind::Show) {
            return Err(malformed());
        }
        Ok(Self {
            provider: provider.parse()?,
            show: parse_canonical(show, false).ok_or_else(malformed)?,
            season: parse_canonical(season, true).ok_or_else(malformed)?,
            episode: parse_canonical(episode, true).ok_or_else(malformed)?,
        })
    }

    /// The public string form.
    pub fn to_storage_string(&self) -> String {
        self.to_string()
    }

    /// The [`MediaId`] of the show this episode belongs to.
    pub fn show_id(&self) -> MediaId {
        MediaId::new(self.provider, MediaKind::Show, self.show)
    }
}

impl fmt::Display for EpisodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:show:{}:{}:{}",
            self.provider, self.show, self.season, self.episode
        )
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
            "tmdb:show:1396",
            "anilist:show:21",
            "anilist:movie:199",
        ] {
            let id = MediaId::parse(raw).expect("parse");
            assert_eq!(id.to_storage_string(), raw);
            assert_eq!(raw.parse::<MediaId>().unwrap(), id);
        }
    }

    #[test]
    fn legacy_tv_kind_is_an_alias_of_show() {
        let id = MediaId::parse("tmdb:tv:1396").unwrap();
        assert_eq!(id.kind, MediaKind::Show);
        assert_eq!(id.to_string(), "tmdb:show:1396");
    }

    #[test]
    fn media_id_exposes_parts() {
        let id = MediaId::parse("tmdb:movie:550").unwrap();
        assert_eq!(id.provider, Provider::Tmdb);
        assert_eq!(id.kind, MediaKind::Movie);
        assert_eq!(id.numeric, 550);
        assert_eq!(id.external_id(), "550");
    }

    #[test]
    fn media_id_from_cameo_provider_id() {
        let id = MediaId::from_cameo("tmdb:1396", MediaKind::Show).unwrap();
        assert_eq!(id, MediaId::new(Provider::Tmdb, MediaKind::Show, 1396));
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
        let raw = "tmdb:show:1396:1:1";
        let id = EpisodeId::parse(raw).expect("parse");
        assert_eq!(id.to_storage_string(), raw);
        assert_eq!(id.show, 1396);
        assert_eq!(id.season, 1);
        assert_eq!(id.episode, 1);
        assert_eq!(
            id.show_id(),
            MediaId::new(Provider::Tmdb, MediaKind::Show, 1396)
        );
        assert_eq!(EpisodeId::parse("tmdb:tv:1396:1:1").unwrap(), id);
    }

    #[test]
    fn episode_id_rejects_bad_input() {
        assert!(EpisodeId::parse("tmdb:movie:1396:1:1").is_err()); // not a show
        assert!(EpisodeId::parse("tmdb:show:1396:1").is_err()); // missing episode
    }

    #[test]
    fn non_canonical_numbers_are_malformed() {
        for raw in [
            "tmdb:movie:+550",
            "tmdb:movie:0550",
            "tmdb:movie:-1",
            "tmdb:movie:0",
            "tmdb:movie: 550",
            "tmdb:movie:",
            "tmdb:movie:2147483648", // past i32::MAX
        ] {
            assert_eq!(
                MediaId::parse(raw),
                Err(MediaIdError::Malformed(raw.to_owned())),
                "{raw}"
            );
        }
        for raw in ["tmdb:+550", "tmdb:0550", "tmdb:-1", "tmdb:0"] {
            assert_eq!(
                MediaId::from_cameo(raw, MediaKind::Movie),
                Err(MediaIdError::Malformed(raw.to_owned())),
                "{raw}"
            );
        }
        for raw in [
            "tmdb:show:01396:1:1",
            "tmdb:show:+1396:1:1",
            "tmdb:show:0:1:1",
            "tmdb:show:1396:01:1",
            "tmdb:show:1396:1:+1",
            "tmdb:show:1396:-0:1",
            "tmdb:show:1396:1:00",
        ] {
            assert_eq!(
                EpisodeId::parse(raw),
                Err(MediaIdError::Malformed(raw.to_owned())),
                "{raw}"
            );
        }
    }

    #[test]
    fn specials_season_and_episode_zero_parse() {
        let id = EpisodeId::parse("tmdb:show:1396:0:0").unwrap();
        assert_eq!((id.season, id.episode), (0, 0));
        assert_eq!(id.to_string(), "tmdb:show:1396:0:0");
    }

    proptest::proptest! {
        #[test]
        fn accepted_media_ids_print_back_canonically(
            s in "(tmdb|anilist):(movie|show|tv):[-+0-9]{1,11}"
        ) {
            // The legacy `tv` alias is accepted as input and parses to the
            // same id as `show`; every other accepted string prints back as-is.
            let canonical = s.replacen(":tv:", ":show:", 1);
            if let Ok(id) = MediaId::parse(&s) {
                proptest::prop_assert_eq!(id.to_string(), canonical.clone());
                proptest::prop_assert_eq!(MediaId::parse(&canonical), Ok(id));
            }
        }

        #[test]
        fn accepted_episode_ids_print_back_canonically(
            s in "(tmdb|anilist):(show|tv):[-+0-9]{1,4}:[-+0-9]{1,3}:[-+0-9]{1,3}"
        ) {
            let canonical = s.replacen(":tv:", ":show:", 1);
            if let Ok(id) = EpisodeId::parse(&s) {
                proptest::prop_assert_eq!(id.to_string(), canonical.clone());
                proptest::prop_assert_eq!(EpisodeId::parse(&canonical), Ok(id));
            }
        }

        #[test]
        fn every_positive_media_id_round_trips(n in 1..=i32::MAX, show in proptest::bool::ANY) {
            let kind = if show { MediaKind::Show } else { MediaKind::Movie };
            let id = MediaId::new(Provider::Tmdb, kind, n);
            proptest::prop_assert_eq!(MediaId::parse(&id.to_string()), Ok(id));
        }

        #[test]
        fn every_episode_id_round_trips(show in 1..=i32::MAX, season: u32, episode: u32) {
            let id = EpisodeId::new(Provider::AniList, show, season, episode);
            proptest::prop_assert_eq!(EpisodeId::parse(&id.to_string()), Ok(id));
        }
    }

    #[test]
    fn ref_provider_conversion() {
        for p in [Provider::Tmdb, Provider::AniList] {
            assert_eq!(Provider::from_ref_provider(p.as_ref_provider()), Some(p));
        }
        assert_eq!(Provider::from_ref_provider(RefProvider::Mal), None);
    }
}
