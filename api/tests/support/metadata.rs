//! A scripted metadata provider.
//!
//! Every title exists unless marked missing. A show has two seasons of three
//! episodes: season 1 aired in 2020, season 2 airs a week from now, a day
//! apart, so up-next and the calendar have something to show. An AniList
//! show has one season of [`ANIME_EPISODES`], as the adapter synthesises it.
//! A provider can be put "down" with any failure class.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{Duration, NaiveDate, Utc};
use stacks_core::entity::enums::{Category, MediaFormat};
use stacks_core::media::anilist;
use stacks_core::media::catalog::MediaSnapshot;
use stacks_core::media::models::{
    EpisodeInfo, Freshness, MediaDetails, MediaWithUserState, SeasonInfo,
};
use stacks_core::media::provider::MetadataFuture;
use stacks_core::media::{
    DiscoverCategory, MediaId, MediaKind, MetadataError, MetadataErrorKind, MetadataPage,
    MetadataProvider, Operation, Provider, SeasonDetails, TitleDetails, UnavailableCause,
};

#[derive(Default)]
struct Script {
    down: HashMap<Provider, MetadataErrorKind>,
    missing: HashSet<MediaId>,
    /// How many season lookups have been made.
    season_calls: usize,
}

/// The mock. Cheap to clone; clones share one script.
#[derive(Clone, Default)]
pub struct MockMetadata(Arc<Mutex<Script>>);

pub const FIGHT_CLUB: &str = "tmdb:movie:550";
pub const BREAKING_BAD: &str = "tmdb:show:1396";
pub const REPLY_1988: &str = "tmdb:show:64010";
pub const ONE_PIECE: &str = "anilist:show:21";

/// An AniList show's episode total: its one season holds episodes 1 to 6,
/// synthesised as the adapter does, with no titles or air dates.
pub const ANIME_EPISODES: u32 = 6;

pub fn id(raw: &str) -> MediaId {
    MediaId::parse(raw).expect("a valid fixture id")
}

/// A provider failure that is a 503 at the API.
pub fn unavailable() -> MetadataErrorKind {
    MetadataErrorKind::Unavailable(UnavailableCause::Status(503))
}

/// The provider refusing Stacks' credential (TMDB's 401): a 502
/// `upstream-misconfigured` at the API.
pub fn misconfigured() -> MetadataErrorKind {
    MetadataErrorKind::Misconfigured("Invalid API key: You must be granted a valid key.".into())
}

/// A provider failure that is a 502 at the API.
pub fn invalid_response() -> MetadataErrorKind {
    MetadataErrorKind::BadUpstreamResponse("explicit null in a required field".into())
}

impl MockMetadata {
    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        self.0.lock().expect("script lock")
    }

    /// Make every call to `provider` fail with `kind`, or recover it.
    pub fn set_down(&self, provider: Provider, kind: Option<MetadataErrorKind>) {
        let mut s = self.script();
        match kind {
            Some(k) => s.down.insert(provider, k),
            None => s.down.remove(&provider),
        };
    }

    /// Make every call to every provider fail with `kind`, or recover them.
    pub fn set_all_down(&self, kind: Option<MetadataErrorKind>) {
        self.set_down(Provider::Tmdb, kind.clone());
        self.set_down(Provider::AniList, kind);
    }

    /// Make a title not exist upstream.
    pub fn set_missing(&self, raw: &str) {
        self.script().missing.insert(id(raw));
    }

    /// How many season lookups have been made so far.
    pub fn season_calls(&self) -> usize {
        self.script().season_calls
    }

    fn check(&self, provider: Provider, op: Operation) -> Result<(), MetadataError> {
        match self.script().down.get(&provider) {
            Some(kind) => Err(MetadataError::new(provider, op, kind.clone())),
            None => Ok(()),
        }
    }

    fn name(id: &MediaId) -> String {
        match id.to_string().as_str() {
            FIGHT_CLUB => "Fight Club".into(),
            BREAKING_BAD => "Breaking Bad".into(),
            REPLY_1988 => "Reply 1988".into(),
            ONE_PIECE => "One Piece".into(),
            _ => format!("Title {}", id.numeric),
        }
    }

    fn origin(id: &MediaId) -> Vec<String> {
        match id.to_string().as_str() {
            REPLY_1988 => vec!["KR".into()],
            ONE_PIECE => vec!["JP".into()],
            _ => vec!["US".into()],
        }
    }

    fn category(id: &MediaId) -> Category {
        match (id.provider, id.kind, id.to_string().as_str()) {
            (Provider::AniList, _, _) => Category::Anime,
            (_, _, REPLY_1988) => Category::KDrama,
            (_, MediaKind::Movie, _) => Category::Movie,
            (_, MediaKind::Show, _) => Category::Series,
        }
    }

    pub fn item(id: &MediaId) -> MediaWithUserState {
        MediaWithUserState {
            cameo_media_id: id.to_string(),
            title: Self::name(id),
            original_title: None,
            romaji_title: None,
            kind: id.kind,
            category: Self::category(id),
            year: Some(2008),
            overview: Some("An overview.".into()),
            poster_url: None,
            backdrop_url: None,
            genres: vec!["Drama".into()],
            rating: Some(8.5),
            episode_count: None,
            watch_status: None,
            rewatching: false,
            score: None,
            is_favorite: false,
        }
    }

    /// Season `n`'s episodes: `(number, air date)`.
    pub fn episodes(season: u32) -> Vec<(u32, NaiveDate)> {
        let start = match season {
            1 => NaiveDate::from_ymd_opt(2020, 1, 6).expect("a date"),
            _ => Utc::now().date_naive() + Duration::days(7),
        };
        (1..=3)
            .map(|n| (n, start + Duration::days(i64::from(n) - 1)))
            .collect()
    }

    fn seasons(id: &MediaId) -> Vec<SeasonInfo> {
        if id.kind != MediaKind::Show {
            return Vec::new();
        }
        if id.provider == Provider::AniList {
            return vec![anilist::season_summary(Some(ANIME_EPISODES))];
        }
        (1..=2)
            .map(|n| SeasonInfo {
                season_number: n,
                name: Some(format!("Season {n}")),
                overview: None,
                air_date: Self::episodes(n).first().map(|(_, d)| d.to_string()),
                poster_url: None,
                episode_count: Some(3),
            })
            .collect()
    }

    fn details_of(&self, id: &MediaId) -> Result<TitleDetails, MetadataError> {
        self.check(id.provider, Operation::Details)?;
        if self.script().missing.contains(id) {
            return Err(MetadataError::new(
                id.provider,
                Operation::Details,
                MetadataErrorKind::NotFound,
            ));
        }
        let seasons = Self::seasons(id);
        let mut snapshot = MediaSnapshot::placeholder(id);
        snapshot.title = Self::name(id);
        snapshot.overview = Some("An overview.".into());
        snapshot.genres = vec!["Drama".into()];
        snapshot.origin_countries = Self::origin(id);
        snapshot.runtime_minutes = Some(if id.kind == MediaKind::Movie { 139 } else { 47 });
        snapshot.season_count = i32::try_from(seasons.len()).ok().filter(|n| *n > 0);
        snapshot.episode_count = (id.kind == MediaKind::Show).then_some(6);
        snapshot.start_date = NaiveDate::from_ymd_opt(2008, 1, 20);
        // What a provider's detail call and AniList's extras settle.
        snapshot.format = Some(match id.kind {
            MediaKind::Movie => MediaFormat::Movie,
            MediaKind::Show => MediaFormat::Tv,
        });
        snapshot.romaji_title = (id.provider == Provider::AniList).then(|| "ONE PIECE".into());
        Ok(TitleDetails {
            detail: MediaDetails {
                media: Self::item(id),
                format: None,
                tagline: Some("A tagline.".into()),
                runtime_minutes: Some(if id.kind == MediaKind::Movie { 139 } else { 47 }),
                status: Some("Ended".into()),
                homepage: None,
                imdb_id: None,
                number_of_seasons: u32::try_from(seasons.len()).ok(),
                in_production: Some(false),
                freshness: Freshness::live(),
            },
            snapshot,
            seasons,
        })
    }

    fn listing(
        &self,
        provider: Provider,
        op: Operation,
        page: u32,
    ) -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        self.check(provider, op)?;
        let ids: &[&str] = match provider {
            Provider::Tmdb => &[FIGHT_CLUB, BREAKING_BAD, REPLY_1988],
            Provider::AniList => &[ONE_PIECE],
        };
        Ok(MetadataPage {
            items: ids.iter().map(|raw| Self::item(&id(raw))).collect(),
            page,
            has_next: page < 2,
            unavailable: Vec::new(),
        })
    }
}

impl MetadataProvider for MockMetadata {
    fn search<'a>(
        &'a self,
        provider: Provider,
        _query: &'a str,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { self.listing(provider, Operation::Search, page) })
    }

    fn details<'a>(&'a self, id: &'a MediaId) -> MetadataFuture<'a, TitleDetails> {
        Box::pin(async move { self.details_of(id) })
    }

    fn season<'a>(&'a self, id: &'a MediaId, season: u32) -> MetadataFuture<'a, SeasonDetails> {
        Box::pin(async move {
            self.script().season_calls += 1;
            self.check(id.provider, Operation::Season)?;
            let seasons = if id.provider == Provider::AniList {
                1..=1
            } else {
                1..=2
            };
            if !seasons.contains(&season) || self.script().missing.contains(id) {
                return Err(MetadataError::new(
                    id.provider,
                    Operation::Season,
                    MetadataErrorKind::NotFound,
                ));
            }
            if id.provider == Provider::AniList {
                return Ok(anilist::season_details(Some(ANIME_EPISODES)));
            }
            Ok(SeasonDetails {
                season: Self::seasons(id)
                    .into_iter()
                    .find(|s| s.season_number == season)
                    .expect("a scripted season"),
                episodes: Self::episodes(season)
                    .into_iter()
                    .map(|(n, aired)| EpisodeInfo {
                        season_number: season,
                        episode_number: n,
                        absolute_number: None,
                        name: Some(format!("Episode {n}")),
                        overview: None,
                        air_date: Some(aired.to_string()),
                        runtime_minutes: Some(47),
                        still_url: None,
                        rating: Some(9.0),
                    })
                    .collect(),
                next_airing: None,
            })
        })
    }

    fn recommendations<'a>(
        &'a self,
        id: &'a MediaId,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { self.listing(id.provider, Operation::Recommendations, page) })
    }

    fn feed<'a>(
        &'a self,
        provider: Provider,
        feed: DiscoverCategory,
        _kind: MediaKind,
        page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move {
            let mut listing = self.listing(provider, Operation::Feed, page)?;
            // A drama feed lists only that region's dramas.
            if let DiscoverCategory::RegionalDrama(region) = feed {
                listing.items.retain(|i| i.category == region.category());
            }
            Ok(listing)
        })
    }
}
