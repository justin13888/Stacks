//! The metadata layer against PostgreSQL 18: reads fall back to the catalog
//! snapshot (marked stale) when a provider fails, multi-provider reads report
//! what is missing, and the `metadata_refresh` job refreshes snapshots,
//! backs off, never deletes data, and announces new episodes.
//!
//! A scripted mock [`MetadataProvider`] drives most tests; one drives the
//! real adapter against a wiremock TMDB that starts returning 500s.

mod common;
#[path = "metadata_fallback/episodes.rs"]
mod episodes;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{Duration as Days, NaiveDate, Utc};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use stacks_core::entity::enums::{EntryStatus, JobType};
use stacks_core::entity::{job, media};
use stacks_core::jobs::handlers::metadata_refresh::{self, MetadataRefreshPayload};
use stacks_core::jobs::{JobContext, JobRunner};
use stacks_core::library::{EntryPatch, LibraryFilter, LibraryService, LibrarySort};
use stacks_core::media::catalog::{self, MediaSnapshot};
use stacks_core::media::models::{
    EpisodeInfo, Freshness, MediaDetails, MediaWithUserState, SeasonInfo,
};
use stacks_core::media::{
    DiscoverCategory, MediaError, MediaId, MediaKind, MediaService, MetadataError,
    MetadataErrorKind, MetadataPage, MetadataProvider, Operation, Provider, SeasonDetails,
    TitleDetails, UnavailableCause, provider::MetadataFuture,
};

// ------------------------------------------------------------------ the mock

/// One scripted show season: episode number and air date.
type Season = Vec<(u32, Option<NaiveDate>)>;

#[derive(Default)]
struct Script {
    /// Providers that fail every call, with the failure.
    down: HashMap<Provider, MetadataErrorKind>,
    /// Titles that do not exist upstream.
    missing: Vec<MediaId>,
    /// The title each id resolves to.
    titles: HashMap<MediaId, String>,
    /// Seasons of each show.
    seasons: HashMap<MediaId, Vec<Season>>,
    /// Season fetches fail while title details still succeed.
    seasons_down: bool,
    calls: usize,
}

#[derive(Default, Clone)]
struct MockProvider(Arc<Mutex<Script>>);

impl MockProvider {
    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        self.0.lock().expect("script lock")
    }

    fn set_down(&self, provider: Provider, kind: Option<MetadataErrorKind>) {
        let mut s = self.script();
        match kind {
            Some(k) => s.down.insert(provider, k),
            None => s.down.remove(&provider),
        };
    }

    fn check(&self, provider: Provider, op: Operation) -> Result<(), MetadataError> {
        let mut s = self.script();
        s.calls += 1;
        match s.down.get(&provider) {
            Some(kind) => Err(MetadataError::new(provider, op, kind.clone())),
            None => Ok(()),
        }
    }

    fn item(id: &MediaId, title: &str) -> MediaWithUserState {
        MediaWithUserState {
            cameo_media_id: id.to_string(),
            title: title.to_owned(),
            original_title: None,
            romaji_title: None,
            kind: id.kind,
            category: stacks_core::entity::enums::Category::Series,
            year: None,
            overview: None,
            poster_url: None,
            backdrop_url: None,
            genres: vec!["Drama".into()],
            rating: None,
            episode_count: None,
            watch_status: None,
            rewatching: false,
            score: None,
            is_favorite: false,
        }
    }

    fn title(&self, id: &MediaId) -> Result<TitleDetails, MetadataError> {
        self.check(id.provider, Operation::Details)?;
        let s = self.script();
        if s.missing.contains(id) {
            return Err(MetadataError::new(
                id.provider,
                Operation::Details,
                MetadataErrorKind::NotFound,
            ));
        }
        let name = s
            .titles
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("Title {}", id.numeric));
        let seasons: Vec<SeasonInfo> = s
            .seasons
            .get(id)
            .map(|ss| {
                ss.iter()
                    .enumerate()
                    .map(|(i, eps)| SeasonInfo {
                        season_number: u32::try_from(i + 1).expect("small"),
                        name: Some(format!("Season {}", i + 1)),
                        overview: None,
                        air_date: None,
                        poster_url: None,
                        episode_count: Some(u32::try_from(eps.len()).expect("small")),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut snapshot = MediaSnapshot::placeholder(id);
        snapshot.title = name.clone();
        snapshot.overview = Some("live overview".into());
        snapshot.season_count = i32::try_from(seasons.len()).ok().filter(|n| *n > 0);
        snapshot.raw = Some(serde_json::json!({ "tagline": "live tagline" }));
        Ok(TitleDetails {
            detail: MediaDetails {
                media: Self::item(id, &name),
                format: None,
                tagline: Some("live tagline".into()),
                runtime_minutes: None,
                status: None,
                homepage: None,
                imdb_id: None,
                number_of_seasons: u32::try_from(seasons.len()).ok(),
                in_production: None,
                freshness: Freshness::live(),
            },
            snapshot,
            seasons,
        })
    }

    fn page(
        &self,
        provider: Provider,
        op: Operation,
    ) -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        self.check(provider, op)?;
        let id = MediaId::new(provider, MediaKind::Show, 7);
        Ok(MetadataPage {
            items: vec![Self::item(&id, &format!("{provider} result"))],
            page: 1,
            has_next: false,
            unavailable: Vec::new(),
        })
    }
}

impl MetadataProvider for MockProvider {
    fn search<'a>(
        &'a self,
        provider: Provider,
        _query: &'a str,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { self.page(provider, Operation::Search) })
    }

    fn details<'a>(&'a self, id: &'a MediaId) -> MetadataFuture<'a, TitleDetails> {
        Box::pin(async move { self.title(id) })
    }

    fn season<'a>(&'a self, id: &'a MediaId, season: u32) -> MetadataFuture<'a, SeasonDetails> {
        Box::pin(async move {
            self.check(id.provider, Operation::Season)?;
            let s = self.script();
            if s.seasons_down {
                return Err(MetadataError::new(
                    id.provider,
                    Operation::Season,
                    unavailable(),
                ));
            }
            let eps = s
                .seasons
                .get(id)
                .and_then(|ss| ss.get(usize::try_from(season).ok()?.checked_sub(1)?))
                .cloned()
                .ok_or_else(|| {
                    MetadataError::new(id.provider, Operation::Season, MetadataErrorKind::NotFound)
                })?;
            Ok(SeasonDetails {
                season: SeasonInfo {
                    season_number: season,
                    name: Some(format!("Season {season}")),
                    overview: None,
                    air_date: None,
                    poster_url: None,
                    episode_count: Some(u32::try_from(eps.len()).expect("small")),
                },
                episodes: eps
                    .iter()
                    .map(|(n, aired)| EpisodeInfo {
                        season_number: season,
                        episode_number: *n,
                        absolute_number: None,
                        name: Some(format!("Episode {n}")),
                        overview: None,
                        air_date: aired.map(|d| d.to_string()),
                        runtime_minutes: Some(30),
                        still_url: None,
                        rating: None,
                    })
                    .collect(),
                next_airing: episodes::next_airing(id, &eps),
            })
        })
    }

    fn recommendations<'a>(
        &'a self,
        id: &'a MediaId,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { self.page(id.provider, Operation::Recommendations) })
    }

    fn feed<'a>(
        &'a self,
        provider: Provider,
        _feed: DiscoverCategory,
        _kind: MediaKind,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { self.page(provider, Operation::Feed) })
    }
}

fn unavailable() -> MetadataErrorKind {
    MetadataErrorKind::Unavailable(UnavailableCause::Status(503))
}

fn service(db: &sea_orm::DatabaseConnection, mock: &MockProvider) -> MediaService {
    MediaService::new(db.clone(), Arc::new(mock.clone()))
}

fn ctx(db: &sea_orm::DatabaseConnection, mock: &MockProvider) -> JobContext {
    JobContext {
        db: db.clone(),
        metadata: Arc::new(mock.clone()),
    }
}

fn days_ago(n: i64) -> Option<NaiveDate> {
    Some(Utc::now().date_naive() - Days::days(n))
}

// ------------------------------------------------------------- read fallback

#[tokio::test]
async fn title_detail_falls_back_to_a_stale_snapshot() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let mock = MockProvider::default();
    let svc = service(db, &mock);
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 550);
    mock.script().titles.insert(id.clone(), "Fight Club".into());

    svc.ensure_in_catalog(&id).await.expect("tracked while up");
    let live = svc.get_detail(user.id, &id).await.expect("live");
    assert!(!live.freshness.stale);
    // The live read refreshed the snapshot.
    let row = catalog::find_by_ref(db, &id).await.unwrap().expect("row");

    mock.set_down(Provider::Tmdb, Some(unavailable()));
    let stale = svc
        .get_detail(user.id, &id)
        .await
        .expect("served from snapshot");
    assert!(stale.freshness.stale);
    assert_eq!(stale.freshness.fetched_at, row.fetched_at);
    assert_eq!(stale.media.title, "Fight Club");
    assert_eq!(
        stale.tagline.as_deref(),
        Some("live tagline"),
        "raw fields survive"
    );

    // Without a snapshot the classified error comes back, not a 500.
    let unknown = MediaId::new(Provider::Tmdb, MediaKind::Movie, 551);
    match svc.get_detail(user.id, &unknown).await {
        Err(MediaError::Metadata(e)) => assert_eq!(e.class(), "unavailable"),
        other => panic!("expected a classified error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_live_detail_refreshes_the_snapshot() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let mock = MockProvider::default();
    let svc = service(db, &mock);
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 1);
    svc.ensure_in_catalog(&id).await.expect("tracked");
    mock.script().titles.insert(id.clone(), "Renamed".into());
    svc.get_detail(user.id, &id).await.expect("live");
    let row = catalog::find_by_ref(db, &id).await.unwrap().expect("row");
    assert_eq!(row.title, "Renamed");
}

#[tokio::test]
async fn seasons_and_episodes_fall_back_to_the_snapshot() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    let svc = service(db, &mock);
    let id = MediaId::new(Provider::Tmdb, MediaKind::Show, 1396);
    mock.script()
        .seasons
        .insert(id.clone(), vec![vec![(1, days_ago(30)), (2, days_ago(23))]]);

    svc.ensure_in_catalog(&id).await.expect("tracked");
    let live = svc.get_episodes(&id, 1).await.expect("live episodes");
    assert_eq!(live.episodes.len(), 2);
    assert!(!live.freshness.stale);

    mock.set_down(Provider::Tmdb, Some(unavailable()));
    let seasons = svc.get_seasons(&id).await.expect("stale seasons");
    assert!(seasons.freshness.stale);
    assert_eq!(seasons.seasons.len(), 1);
    assert_eq!(seasons.seasons[0].episode_count, Some(2));
    let episodes = svc.get_episodes(&id, 1).await.expect("stale episodes");
    assert!(episodes.freshness.stale);
    assert_eq!(
        episodes
            .episodes
            .iter()
            .map(|e| e.episode_number)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    // A season the snapshot never saw is the classified error.
    assert!(matches!(
        svc.get_episodes(&id, 2).await,
        Err(MediaError::Metadata(_))
    ));
}

#[tokio::test]
async fn search_degrades_to_the_providers_that_answered() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let mock = MockProvider::default();
    let svc = service(db, &mock);
    let both = svc.search(user.id, "x", 1).await.expect("both up");
    assert_eq!(both.items.len(), 2);
    assert!(!both.is_partial());

    mock.set_down(
        Provider::AniList,
        Some(MetadataErrorKind::RateLimited { retry_after: None }),
    );
    let partial = svc
        .search(user.id, "x", 1)
        .await
        .expect("TMDB still answers");
    assert_eq!(partial.items.len(), 1);
    assert!(partial.is_partial());
    assert_eq!(partial.degraded.len(), 1);
    assert_eq!(partial.degraded[0].provider, Provider::AniList);
    assert_eq!(partial.degraded[0].class(), "rate_limited");

    mock.set_down(Provider::Tmdb, Some(unavailable()));
    assert!(matches!(
        svc.search(user.id, "x", 1).await,
        Err(MediaError::Metadata(_))
    ));
}

#[tokio::test]
async fn tracking_while_down_stores_a_placeholder() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    let svc = service(db, &mock);
    mock.set_down(Provider::Tmdb, Some(unavailable()));
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 77);
    let row = svc.ensure_in_catalog(&id).await.expect("placeholder");
    assert_eq!(row.refresh_failures, 1);
    assert!(
        row.refresh_error
            .as_deref()
            .is_some_and(|e| e.contains("unavailable"))
    );
    assert!(row.fetched_at.is_none());

    // A title the provider says does not exist is not stored.
    mock.set_down(Provider::Tmdb, None);
    let gone = MediaId::new(Provider::Tmdb, MediaKind::Movie, 78);
    mock.script().missing.push(gone.clone());
    assert!(matches!(
        svc.ensure_in_catalog(&gone).await,
        Err(MediaError::NotFound(_))
    ));
    assert!(catalog::find_by_ref(db, &gone).await.unwrap().is_none());
}

/// The acceptance scenario with the real adapter: TMDB starts returning 500s
/// and both title detail and the library still render from the snapshot.
#[tokio::test]
async fn tmdb_500s_still_render_detail_and_library_from_the_snapshot() {
    use stacks_core::media::provider::{RateSettings, RetrySettings};
    use stacks_core::media::{CameoProvider, ProviderSettings};
    use wiremock::matchers::{any, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let tmdb = MockServer::start().await;
    Mock::given(path("/3/movie/550"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 550, "title": "Fight Club", "original_title": "Fight Club",
            "overview": "x", "release_date": "1999-10-15", "genres": [],
            "original_language": "en", "runtime": 139, "status": "Released",
            "tagline": "Mischief. Mayhem. Soap.", "imdb_id": "tt0137523",
            "adult": false, "video": false, "vote_average": 8.4, "vote_count": 1,
            "popularity": 1.0, "production_companies": [], "production_countries": [],
            "spoken_languages": []
        })))
        .up_to_n_times(1)
        .mount(&tmdb)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .mount(&tmdb)
        .await;
    let settings = ProviderSettings {
        retry: RetrySettings {
            max_retries: 0,
            ..ProviderSettings::default().retry
        },
        tmdb_rate: RateSettings {
            max_requests: 1000,
            per: std::time::Duration::from_secs(1),
        },
        tmdb_base_url: Some(tmdb.uri()),
        anilist_base_url: Some(tmdb.uri()),
        tmdb_language: None,
        ..ProviderSettings::default()
    };
    let metadata: Arc<dyn MetadataProvider> =
        Arc::new(CameoProvider::new("0123456789abcdef0123456789abcdef", &settings).unwrap());
    let svc = MediaService::new(db.clone(), metadata);
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 550);

    let row = svc
        .ensure_in_catalog(&id)
        .await
        .expect("first call succeeds");
    assert_eq!(row.title, "Fight Club");
    LibraryService::new(db.clone())
        .upsert(
            user.id,
            row.id,
            EntryPatch {
                status: Some(EntryStatus::Completed),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let detail = svc.get_detail(user.id, &id).await.expect("stale detail");
    assert!(detail.freshness.stale);
    assert_eq!(detail.media.title, "Fight Club");
    assert_eq!(detail.media.watch_status, Some(EntryStatus::Completed));
    assert_eq!(detail.tagline.as_deref(), Some("Mischief. Mayhem. Soap."));

    let library = LibraryService::new(db.clone())
        .list(
            user.id,
            &LibraryFilter::default(),
            LibrarySort::default(),
            None,
            50,
        )
        .await
        .expect("the library reads the snapshot");
    assert_eq!(library.items.len(), 1);
    assert_eq!(library.items[0].media.title, "Fight Club");
}

// ---------------------------------------------------------- the refresh job

#[tokio::test]
async fn a_failed_refresh_keeps_the_data_and_records_the_failure() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 10);
    mock.script().titles.insert(id.clone(), "Kept".into());
    let row = service(db, &mock).ensure_in_catalog(&id).await.unwrap();

    mock.set_down(Provider::Tmdb, Some(unavailable()));
    let payload = MetadataRefreshPayload {
        media_ids: vec![row.id],
        batch_size: None,
    };
    let report = metadata_refresh::run(&ctx(db, &mock), payload.clone())
        .await
        .expect("a provider failure does not fail the job");
    assert_eq!(report.failed, 1);
    assert_eq!(report.refreshed, 0);
    let after = media::Entity::find_by_id(row.id)
        .one(db)
        .await
        .unwrap()
        .expect("not deleted");
    assert_eq!(after.title, "Kept");
    assert_eq!(after.overview, row.overview);
    assert_eq!(after.fetched_at, row.fetched_at);
    assert_eq!(after.refresh_failures, 1);
    assert!(after.refresh_error.is_some());

    metadata_refresh::run(&ctx(db, &mock), payload.clone())
        .await
        .unwrap();
    let after = media::Entity::find_by_id(row.id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.refresh_failures, 2);

    // Recovery clears the failure bookkeeping.
    mock.set_down(Provider::Tmdb, None);
    mock.script().titles.insert(id.clone(), "Fresh".into());
    let report = metadata_refresh::run(&ctx(db, &mock), payload)
        .await
        .unwrap();
    assert_eq!(report.refreshed, 1);
    let after = media::Entity::find_by_id(row.id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.title, "Fresh");
    assert_eq!(after.refresh_failures, 0);
    assert!(after.refresh_error.is_none());
}

#[tokio::test]
async fn stale_selection_backs_off_failing_titles() {
    let t = test_db!();
    let db = &t.db;
    let fresh = common::movie(db, 1).await;
    let stale = common::movie(db, 2).await;
    let failing = common::movie(db, 3).await;
    db.execute_unprepared(&format!(
        "UPDATE media SET fetched_at = now() - interval '2 days' WHERE id IN ('{}', '{}');
         UPDATE media SET refresh_failures = 3 WHERE id = '{}';",
        stale.id, failing.id, failing.id
    ))
    .await
    .unwrap();
    let due: Vec<_> = catalog::stale_snapshots(db, metadata_refresh::FRESH_FOR_SECS, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(due, vec![stale.id], "fresh and recently failed titles wait");
    assert!(!due.contains(&fresh.id));

    // Once its backoff (1h × 2^3) has passed, the failing title is due again.
    db.execute_unprepared(&format!(
        "ALTER TABLE media DISABLE TRIGGER USER;
         UPDATE media SET updated_at = now() - interval '9 hours' WHERE id = '{}';
         ALTER TABLE media ENABLE TRIGGER USER;",
        failing.id
    ))
    .await
    .unwrap();
    let due = catalog::stale_snapshots(db, metadata_refresh::FRESH_FOR_SECS, 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 2);
}

#[tokio::test]
async fn the_runner_queues_one_refresh_at_a_time_and_runs_it() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    let runner = JobRunner::new(ctx(db, &mock));
    assert!(runner.schedule_refresh().await.unwrap());
    assert!(!runner.schedule_refresh().await.unwrap(), "already queued");

    let stale = common::movie(db, 5).await;
    db.execute_unprepared(&format!(
        "UPDATE media SET fetched_at = NULL WHERE id = '{}'",
        stale.id
    ))
    .await
    .unwrap();
    assert_eq!(runner.run_once().await.unwrap(), 1);
    let done = job::Entity::find()
        .filter(job::Column::JobType.eq(JobType::MetadataRefresh))
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        done.status,
        stacks_core::entity::enums::JobStatus::Completed
    );
    let refreshed = media::Entity::find_by_id(stale.id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert!(refreshed.fetched_at.is_some());
    assert_eq!(refreshed.title, "Title 5");
    assert!(
        runner.schedule_refresh().await.unwrap(),
        "a new one once done"
    );
}

#[tokio::test]
async fn a_refresh_stranded_by_a_restart_does_not_stop_the_schedule() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    let runner = JobRunner::new(ctx(db, &mock));
    assert!(runner.schedule_refresh().await.unwrap());

    // A worker claims the refresh and the process dies mid-batch.
    let claimed = stacks_core::jobs::repo::claim_due_jobs(db, 10)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    db.execute_unprepared(&format!(
        "UPDATE jobs SET started_at = now() - make_interval(secs => {}) WHERE id = '{}'",
        stacks_core::jobs::runner::STALE_RUNNING_AFTER_SECS + 60,
        claimed[0].id
    ))
    .await
    .unwrap();
    assert!(
        !runner.schedule_refresh().await.unwrap(),
        "the stranded row still looks queued until it is reclaimed"
    );

    // The next tick reclaims it (its one attempt is spent, so it fails), and
    // the schedule can queue a fresh refresh, which runs.
    runner.run_once().await.unwrap();
    let stranded = job::Entity::find_by_id(claimed[0].id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stranded.status,
        stacks_core::entity::enums::JobStatus::Failed
    );
    assert!(runner.schedule_refresh().await.unwrap());
    assert_eq!(runner.run_once().await.unwrap(), 1);
}
