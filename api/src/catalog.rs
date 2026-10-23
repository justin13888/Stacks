//! Browsing the catalog: search, title detail, seasons, recommendations and
//! discovery feeds.
//!
//! Every read goes to the metadata providers (TMDB, AniList) and falls back
//! to the stored catalog snapshot when they fail, marked `stale`. A listing
//! assembled from several provider calls names the calls that failed in
//! `degraded` rather than hiding them, and is an error only when every call
//! failed.

use std::sync::Arc;

use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::blocklist::BlocklistService;
use stacks_core::entity::enums::{Category as CoreCategory, EntryStatus as CoreStatus};
use stacks_core::media::models::{
    EpisodeInfo, Freshness as CoreFreshness, Listing, MediaDetails, MediaWithUserState, SeasonInfo,
};
use stacks_core::media::{
    DiscoverCategory, MediaId, MediaKind, MediaService, MetadataError, MetadataProvider, Provider,
};

use crate::app::App;
use crate::ids::TitleRef;
use crate::page::{self, InvalidCursor};
use crate::problem::{CatalogError, Internal, TitleNotFound, Upstream};
use crate::security::{Caller, Reader};

/// Browsing titles.
#[derive(Tag)]
#[tag(
    name = "catalog",
    description = "Search, title detail, seasons, recommendations and discovery feeds."
)]
pub struct Catalog;

/// Whether a title is a single film or an episodic show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Movie,
    Show,
}

impl From<MediaKind> for Kind {
    fn from(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Movie => Self::Movie,
            MediaKind::Show => Self::Show,
        }
    }
}

impl From<Kind> for MediaKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Movie => Self::Movie,
            Kind::Show => Self::Show,
        }
    }
}

/// The browsing category, derived from provider, format, origin country and
/// genre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Movie,
    Series,
    Anime,
    KDrama,
    JDrama,
    CDrama,
    TwDrama,
    ThDrama,
}

impl From<CoreCategory> for Category {
    fn from(c: CoreCategory) -> Self {
        match c {
            CoreCategory::Movie => Self::Movie,
            CoreCategory::Series => Self::Series,
            CoreCategory::Anime => Self::Anime,
            CoreCategory::KDrama => Self::KDrama,
            CoreCategory::JDrama => Self::JDrama,
            CoreCategory::CDrama => Self::CDrama,
            CoreCategory::TwDrama => Self::TwDrama,
            CoreCategory::ThDrama => Self::ThDrama,
        }
    }
}

impl From<Category> for CoreCategory {
    fn from(c: Category) -> Self {
        match c {
            Category::Movie => Self::Movie,
            Category::Series => Self::Series,
            Category::Anime => Self::Anime,
            Category::KDrama => Self::KDrama,
            Category::JDrama => Self::JDrama,
            Category::CDrama => Self::CDrama,
            Category::TwDrama => Self::TwDrama,
            Category::ThDrama => Self::ThDrama,
        }
    }
}

/// Where a library entry stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planning,
    Watching,
    Completed,
    Paused,
    Dropped,
}

impl From<CoreStatus> for Status {
    fn from(s: CoreStatus) -> Self {
        match s {
            CoreStatus::Planning => Self::Planning,
            CoreStatus::Watching => Self::Watching,
            CoreStatus::Completed => Self::Completed,
            CoreStatus::Paused => Self::Paused,
            CoreStatus::Dropped => Self::Dropped,
        }
    }
}

impl From<Status> for CoreStatus {
    fn from(s: Status) -> Self {
        match s {
            Status::Planning => Self::Planning,
            Status::Watching => Self::Watching,
            Status::Completed => Self::Completed,
            Status::Paused => Self::Paused,
            Status::Dropped => Self::Dropped,
        }
    }
}

/// The caller's relationship with a title, when it is in their library.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct LibraryState {
    pub status: Status,
    /// 0–100, or `null` when unscored.
    pub score: Option<i32>,
    pub favourite: bool,
    /// Whether the caller is on a rewatch.
    pub rewatching: bool,
}

/// A title, as listings show it.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct TitleSummary {
    /// The title's reference, which every title-scoped operation takes.
    #[serde(rename = "ref")]
    pub reference: TitleRef,
    pub title: String,
    /// The original-language title, when it differs from `title`.
    pub original_title: Option<String>,
    pub kind: Kind,
    pub category: Category,
    /// The release year, or the first-air year of a show.
    pub year: Option<i32>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub genres: Vec<String>,
    /// The provider's rating out of 10, when it has one.
    pub rating: Option<f64>,
    /// `null` when the title is not in the caller's library.
    pub library: Option<LibraryState>,
}

impl TitleSummary {
    /// The summary of a provider item, or `None` (logged) for an item whose
    /// reference does not parse, which the provider adapter never produces.
    pub fn from_media(m: MediaWithUserState) -> Option<Self> {
        let reference = match m.cameo_media_id.parse::<TitleRef>() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(reference = %m.cameo_media_id, error = %e, "unparseable title reference");
                return None;
            }
        };
        let library = m.watch_status.map(|status| LibraryState {
            status: status.into(),
            score: m.score,
            favourite: m.is_favorite,
            rewatching: m.rewatching,
        });
        Some(Self {
            reference,
            title: m.title,
            original_title: m.original_title,
            kind: m.kind.into(),
            category: m.category.into(),
            year: m.year,
            overview: m.overview,
            poster_url: m.poster_url,
            backdrop_url: m.backdrop_url,
            genres: m.genres,
            rating: m.rating,
            library,
        })
    }
}

/// A metadata provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataSource {
    Tmdb,
    Anilist,
}

impl From<Provider> for MetadataSource {
    fn from(p: Provider) -> Self {
        match p {
            Provider::Tmdb => Self::Tmdb,
            Provider::AniList => Self::Anilist,
        }
    }
}

/// One provider call that failed while a listing was assembled.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Degradation {
    pub provider: MetadataSource,
    /// The failure's class: `rate_limited`, `unavailable`, `misconfigured`,
    /// `bad_upstream_response` or `unsupported`.
    pub class: String,
}

impl From<&MetadataError> for Degradation {
    fn from(e: &MetadataError) -> Self {
        Self {
            provider: e.provider.into(),
            class: e.class().to_owned(),
        }
    }
}

/// One page of titles assembled from the metadata providers.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct TitlePage {
    pub items: Vec<TitleSummary>,
    /// Pass as `cursor` to fetch the next page; `null` on the last page.
    pub next_cursor: Option<String>,
    /// The provider calls that failed; the items are what the rest returned.
    pub degraded: Vec<Degradation>,
    /// How many upstream entries could not be read and are missing from
    /// `items`.
    pub unreadable: u32,
}

impl TitlePage {
    fn from_listing(
        listing: Listing<MediaWithUserState>,
        keep: impl Fn(&TitleSummary) -> bool,
    ) -> Self {
        let degraded = listing.degraded.iter().map(Degradation::from).collect();
        Self {
            items: listing
                .items
                .into_iter()
                .filter_map(TitleSummary::from_media)
                .filter(|t| keep(t))
                .collect(),
            next_cursor: listing.next_page.map(|p| page::encode(&p)),
            degraded,
            unreadable: u32::try_from(listing.unavailable.len()).unwrap_or(u32::MAX),
        }
    }
}

/// Whether a read came live from the provider or from the stored snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct Freshness {
    /// `true` when the provider failed and the stored snapshot was served.
    pub stale: bool,
    /// When the data was last fetched from the provider, if ever.
    pub fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<CoreFreshness> for Freshness {
    fn from(f: CoreFreshness) -> Self {
        Self {
            stale: f.stale,
            fetched_at: f.fetched_at.map(|t| t.to_utc()),
        }
    }
}

/// Everything known about one title.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct TitleDetail {
    #[serde(flatten)]
    pub summary: TitleSummary,
    pub tagline: Option<String>,
    /// The film's runtime, or a show's typical episode runtime, in minutes.
    pub runtime_minutes: Option<u32>,
    /// The release status, as the provider words it.
    pub release_status: Option<String>,
    pub homepage: Option<String>,
    pub imdb_id: Option<String>,
    /// Shows only.
    pub season_count: Option<u32>,
    /// Shows only.
    pub episode_count: Option<u32>,
    /// Shows only: whether new episodes are still being made.
    pub in_production: Option<bool>,
    pub freshness: Freshness,
}

impl TitleDetail {
    fn from_details(d: MediaDetails) -> Option<Self> {
        Some(Self {
            summary: TitleSummary::from_media(d.media)?,
            tagline: d.tagline,
            runtime_minutes: d.runtime_minutes,
            release_status: d.status,
            homepage: d.homepage,
            imdb_id: d.imdb_id,
            season_count: d.number_of_seasons,
            episode_count: d.number_of_episodes,
            in_production: d.in_production,
            freshness: d.freshness.into(),
        })
    }
}

/// A provider date (`YYYY-MM-DD`); anything else reads as unknown.
fn date(raw: Option<&str>) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(raw?, "%Y-%m-%d").ok()
}

/// One season of a show. Specials are season 0.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Season {
    pub number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<chrono::NaiveDate>,
    pub poster_url: Option<String>,
    /// `null` when the provider did not say.
    pub episode_count: Option<u32>,
}

impl From<SeasonInfo> for Season {
    fn from(s: SeasonInfo) -> Self {
        Self {
            number: s.season_number,
            name: s.name,
            overview: s.overview,
            air_date: date(s.air_date.as_deref()),
            poster_url: s.poster_url,
            episode_count: s.episode_count,
        }
    }
}

/// A show's seasons.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Seasons {
    pub items: Vec<Season>,
    pub freshness: Freshness,
}

/// One episode of a show.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Episode {
    pub season: u32,
    pub number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<chrono::NaiveDate>,
    pub runtime_minutes: Option<u32>,
    pub still_url: Option<String>,
    /// The provider's rating out of 10, when it has one.
    pub rating: Option<f64>,
}

impl From<EpisodeInfo> for Episode {
    fn from(e: EpisodeInfo) -> Self {
        Self {
            season: e.season_number,
            number: e.episode_number,
            name: e.name,
            overview: e.overview,
            air_date: date(e.air_date.as_deref()),
            runtime_minutes: e.runtime_minutes,
            still_url: e.still_url,
            rating: e.rating,
        }
    }
}

/// One season with its episodes.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct SeasonDetail {
    pub number: u32,
    pub episodes: Vec<Episode>,
    pub freshness: Freshness,
}

// ------------------------------------------------------------- parameters

/// How the catalog is searched.
#[derive(Debug, Schema, QueryParams)]
pub struct SearchParams {
    /// The text to search for.
    #[schema(min_length = 1, max_length = 200)]
    pub q: String,
    /// Only movies, or only shows.
    pub kind: Option<Kind>,
    /// Only this category.
    pub category: Option<Category>,
    /// The `next_cursor` of the previous page. Omit for the first page.
    pub cursor: Option<String>,
}

/// What `/titles/{ref}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct TitlePath {
    /// The title's reference, such as `tmdb:movie:550`.
    #[param(rename = "ref")]
    pub reference: TitleRef,
}

/// What `/titles/{ref}/seasons/{number}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct SeasonPath {
    /// The show's reference, such as `tmdb:show:1396`.
    #[param(rename = "ref")]
    pub reference: TitleRef,
    /// The season number; specials are season 0.
    #[schema(maximum = 10000)]
    pub number: u32,
}

/// Where a listing continues.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct CursorParams {
    /// The `next_cursor` of the previous page. Omit for the first page.
    pub cursor: Option<String>,
}

/// A discovery feed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Schema, Serialize, Deserialize, kynos::ParamValue)]
#[serde(rename_all = "kebab-case")]
pub enum Feed {
    /// Trending on TMDB this week.
    Trending,
    /// Popular on TMDB.
    Popular,
    /// Highest rated on TMDB.
    TopRated,
    /// Trending anime on AniList.
    TrendingAnime,
    /// Popular anime on AniList.
    PopularAnime,
    /// Highest rated anime on AniList.
    TopRatedAnime,
}

impl Feed {
    fn source(self) -> (Provider, DiscoverCategory) {
        match self {
            Self::Trending => (Provider::Tmdb, DiscoverCategory::Trending),
            Self::Popular => (Provider::Tmdb, DiscoverCategory::Popular),
            Self::TopRated => (Provider::Tmdb, DiscoverCategory::TopRated),
            Self::TrendingAnime => (Provider::AniList, DiscoverCategory::Trending),
            Self::PopularAnime => (Provider::AniList, DiscoverCategory::Popular),
            Self::TopRatedAnime => (Provider::AniList, DiscoverCategory::TopRated),
        }
    }

    /// What a feed lists when the request names no kind: films for the
    /// general feeds, series for the anime ones.
    fn default_kind(self) -> MediaKind {
        match self.source().0 {
            Provider::Tmdb => MediaKind::Movie,
            Provider::AniList => MediaKind::Show,
        }
    }
}

/// What `/feeds/{feed}` captures.
#[derive(Debug, Schema, PathParams)]
pub struct FeedPath {
    pub feed: Feed,
}

/// How a feed is read.
#[derive(Debug, Schema, QueryParams)]
pub struct FeedParams {
    /// Movies or shows. Defaults to movies for the general feeds and shows
    /// for the anime ones.
    pub kind: Option<Kind>,
    /// The `next_cursor` of the previous page. Omit for the first page.
    pub cursor: Option<String>,
}

// ----------------------------------------------------------------- errors

/// What a catalog listing can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListingError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Upstream(#[from] Upstream),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

impl From<CatalogError> for ListingError {
    fn from(e: CatalogError) -> Self {
        match e {
            CatalogError::Upstream(u) => u.into(),
            CatalogError::Internal(i) => i.into(),
            // A listing has no title of its own to miss.
            CatalogError::NotFound(TitleNotFound(what)) => Internal::log(what).into(),
        }
    }
}

/// What reading a title's recommendations can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum RecommendationsError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Catalog(#[from] CatalogError),
}

/// The provider page a catalog cursor names; page 1 without one.
fn page_number(cursor: Option<&str>) -> Result<u32, InvalidCursor> {
    match cursor {
        None => Ok(1),
        Some(c) => {
            page::decode::<u32>(c).and_then(|p| if p >= 1 { Ok(p) } else { Err(InvalidCursor) })
        }
    }
}

/// Drop the caller's blocked titles from a listing.
async fn without_blocked(
    db: &DatabaseConnection,
    caller: &Caller,
    listing: Listing<MediaWithUserState>,
) -> Result<Listing<MediaWithUserState>, Internal> {
    let blocked = BlocklistService::new(db.clone())
        .blocked_refs(caller.id())
        .await
        .map_err(Internal::from)?;
    let items = listing
        .items
        .into_iter()
        .filter(|m| MediaId::parse(&m.cameo_media_id).map_or(true, |id| !blocked.contains(&id)))
        .collect();
    Ok(Listing { items, ..listing })
}

// ------------------------------------------------------------- operations

/// Search movies, shows and anime.
///
/// Searches every provider at once. Blocked titles are left out.
#[kynos::get("/search", operation_id = "searchTitles")]
async fn search_titles(
    Auth(caller): Reader,
    Query(params): Query<SearchParams>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<Json<TitlePage>, ListingError> {
    let caller = Caller::from(caller);
    let page = page_number(params.cursor.as_deref())?;
    let listing = MediaService::new(db.clone(), metadata)
        .search(caller.id(), params.q.trim(), page)
        .await
        .map_err(CatalogError::from)?;
    let listing = without_blocked(&db, &caller, listing).await?;
    Ok(Json(TitlePage::from_listing(listing, |t| {
        params.kind.is_none_or(|k| t.kind == k) && params.category.is_none_or(|c| t.category == c)
    })))
}

/// Read a title.
///
/// Live from its provider; the stored snapshot, with `freshness.stale`, when
/// the provider is failing.
#[kynos::get("/titles/{ref}", operation_id = "getTitle")]
async fn get_title(
    Auth(caller): Reader,
    Path(path): Path<TitlePath>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<Json<TitleDetail>, CatalogError> {
    let caller = Caller::from(caller);
    let detail = MediaService::new(db, metadata)
        .get_detail(caller.id(), path.reference.media_id())
        .await?;
    TitleDetail::from_details(detail)
        .map(Json)
        .ok_or_else(|| Internal::log("a title detail without a reference").into())
}

/// List a show's seasons.
#[kynos::get("/titles/{ref}/seasons", operation_id = "listSeasons")]
async fn list_seasons(
    _caller: Reader,
    Path(path): Path<TitlePath>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<Json<Seasons>, CatalogError> {
    let seasons = MediaService::new(db, metadata)
        .get_seasons(path.reference.media_id())
        .await?;
    Ok(Json(Seasons {
        items: seasons.seasons.into_iter().map(Season::from).collect(),
        freshness: seasons.freshness.into(),
    }))
}

/// Read one season of a show, with its episodes.
#[kynos::get("/titles/{ref}/seasons/{number}", operation_id = "getSeason")]
async fn get_season(
    _caller: Reader,
    Path(path): Path<SeasonPath>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<Json<SeasonDetail>, CatalogError> {
    let episodes = MediaService::new(db, metadata)
        .get_episodes(path.reference.media_id(), path.number)
        .await?;
    Ok(Json(SeasonDetail {
        number: path.number,
        episodes: episodes.episodes.into_iter().map(Episode::from).collect(),
        freshness: episodes.freshness.into(),
    }))
}

/// List titles like this one.
#[kynos::get("/titles/{ref}/recommendations", operation_id = "listRecommendations")]
async fn list_recommendations(
    Auth(caller): Reader,
    Path(path): Path<TitlePath>,
    Query(params): Query<CursorParams>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<Json<TitlePage>, RecommendationsError> {
    let caller = Caller::from(caller);
    let page = page_number(params.cursor.as_deref())?;
    let listing = MediaService::new(db.clone(), metadata)
        .get_recommendations(caller.id(), path.reference.media_id(), page)
        .await
        .map_err(CatalogError::from)?;
    let listing = without_blocked(&db, &caller, listing)
        .await
        .map_err(CatalogError::from)?;
    Ok(Json(TitlePage::from_listing(listing, |_| true)))
}

/// Read a discovery feed.
///
/// Blocked titles are left out.
#[kynos::get("/feeds/{feed}", operation_id = "getFeed")]
async fn get_feed(
    Auth(caller): Reader,
    Path(path): Path<FeedPath>,
    Query(params): Query<FeedParams>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<Json<TitlePage>, ListingError> {
    let caller = Caller::from(caller);
    let page = page_number(params.cursor.as_deref())?;
    let (provider, category) = path.feed.source();
    let kind = params
        .kind
        .map_or_else(|| path.feed.default_kind(), MediaKind::from);
    let listing = MediaService::new(db.clone(), metadata)
        .discover(caller.id(), provider, category, kind, page)
        .await
        .map_err(CatalogError::from)?;
    let listing = without_blocked(&db, &caller, listing).await?;
    Ok(Json(TitlePage::from_listing(listing, |_| true)))
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Catalog>().mount(kynos::routes![
        search_titles,
        get_title,
        list_seasons,
        get_season,
        list_recommendations,
        get_feed,
    ])
}
