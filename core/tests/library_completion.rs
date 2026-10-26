//! Logging the plays that finish a show completes it, against PostgreSQL 18:
//! only a finished or cancelled show, only once every regular-season episode
//! has a live play in the current run, dated from the play that finished it,
//! and never over a status the user chose after the run was already
//! finished. Covers TMDB shows and an AniList show AniList calls FINISHED.

mod common;

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDate};
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder};
use serde_json::{Value, json};
use stacks_core::Patch;
use stacks_core::entity::enums::{
    DatePrecision, EntryEventKind, EntryStatus, MediaKind, PlaySource, ReleaseStatus,
};
use stacks_core::entity::{entry_event, media};
use stacks_core::library::{EntryPatch, LibraryService, LogPlay, PlayBatch, RunDate, RunPatch};
use stacks_core::media::catalog;
use stacks_core::media::models::SeasonInfo;
use stacks_core::media::provider::RetrySettings;
use stacks_core::media::{
    CameoProvider, MediaId, MediaService, MetadataProvider, Provider, ProviderSettings,
};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn at(rfc3339: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(rfc3339).unwrap()
}

/// A TMDB show with `status`, `season_count` and the given season summaries
/// (`(number, episode count)`).
async fn show(
    db: &DatabaseConnection,
    numeric: i32,
    status: Option<ReleaseStatus>,
    season_count: Option<i32>,
    seasons: &[(u32, Option<u32>)],
) -> media::Model {
    let mut snap = common::snapshot(MediaKind::Show, numeric, "A Show");
    snap.status = status;
    snap.season_count = season_count;
    let row = catalog::upsert_snapshot(db, &snap).await.unwrap();
    let seasons: Vec<SeasonInfo> = seasons
        .iter()
        .map(|&(n, count)| SeasonInfo {
            season_number: n,
            name: None,
            overview: None,
            air_date: None,
            poster_url: None,
            episode_count: count,
        })
        .collect();
    catalog::upsert_seasons(db, row.id, &seasons).await.unwrap();
    row
}

/// An ended show: specials (one), then two seasons of two episodes.
async fn ended(db: &DatabaseConnection, numeric: i32, status: ReleaseStatus) -> media::Model {
    show(
        db,
        numeric,
        Some(status),
        Some(2),
        &[(0, Some(1)), (1, Some(2)), (2, Some(2))],
    )
    .await
}

async fn log(
    svc: &LibraryService,
    user: uuid::Uuid,
    show: &media::Model,
    episodes: &[(i32, i32)],
    watched_at: Option<DateTime<FixedOffset>>,
) {
    svc.log_plays(
        user,
        show,
        PlayBatch::Episodes(episodes.to_vec()),
        watched_at,
        PlaySource::Manual,
        None,
    )
    .await
    .unwrap();
}

async fn status(svc: &LibraryService, user: uuid::Uuid, show: &media::Model) -> EntryStatus {
    svc.get(user, show.id).await.unwrap().unwrap().entry.status
}

/// The kinds of the events written after the `skip` first ones.
async fn kinds_after(db: &DatabaseConnection, skip: usize) -> Vec<EntryEventKind> {
    entry_event::Entity::find()
        .order_by_asc(entry_event::Column::Id)
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .skip(skip)
        .map(|e| e.kind)
        .collect()
}

async fn event_count(db: &DatabaseConnection) -> usize {
    usize::try_from(common::count(db, "entry_events").await).unwrap()
}

#[tokio::test]
async fn the_last_regular_episode_of_an_ended_show_completes_it() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = ended(db, 300, ReleaseStatus::Finished).await;
    let svc = LibraryService::new(db.clone());

    let march_1 = RunDate {
        date: NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
        precision: DatePrecision::Day,
    };
    svc.log_plays_on(
        user.id,
        &show,
        PlayBatch::Episodes(vec![(1, 1), (1, 2), (2, 1)]),
        Some(at("2024-03-01T20:00:00Z")),
        Some(march_1),
        PlaySource::Manual,
    )
    .await
    .unwrap();
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Watching);

    // The last regular episode finishes the show, though the special is
    // unwatched. It carries no calendar date, so the run completes on the
    // UTC date it was watched.
    let before = event_count(db).await;
    log(
        &svc,
        user.id,
        &show,
        &[(2, 2)],
        Some(at("2024-03-05T21:30:00+00:00")),
    )
    .await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
    let runs = svc.runs(user.id, show.id).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].completed_on, NaiveDate::from_ymd_opt(2024, 3, 5));
    assert_eq!(runs[0].completed_precision, Some(DatePrecision::Day));
    assert_eq!(
        kinds_after(db, before).await,
        [
            EntryEventKind::PlayLogged,
            EntryEventKind::EntryUpdated,
            EntryEventKind::RunUpdated,
        ]
    );
}

#[tokio::test]
async fn a_cancelled_show_completes_and_an_undated_play_completes_it_today() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = ended(db, 301, ReleaseStatus::Cancelled).await;
    let svc = LibraryService::new(db.clone());

    log(
        &svc,
        user.id,
        &show,
        &[(1, 1), (1, 2), (2, 1), (2, 2)],
        None,
    )
    .await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
    let runs = svc.runs(user.id, show.id).await.unwrap();
    assert_eq!(runs[0].completed_on, Some(chrono::Utc::now().date_naive()));
}

#[tokio::test]
async fn a_show_still_airing_or_of_unknown_status_stays_watching() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = LibraryService::new(db.clone());
    let all = [(1, 1), (1, 2), (2, 1), (2, 2)];
    for (n, status_) in [
        (310, Some(ReleaseStatus::Releasing)),
        (311, Some(ReleaseStatus::Hiatus)),
        (312, Some(ReleaseStatus::Upcoming)),
        (313, None),
    ] {
        let show = show(db, n, status_, Some(2), &[(1, Some(2)), (2, Some(2))]).await;
        log(&svc, user.id, &show, &all, None).await;
        assert_eq!(
            status(&svc, user.id, &show).await,
            EntryStatus::Watching,
            "{status_:?}"
        );
        assert_eq!(
            svc.runs(user.id, show.id).await.unwrap()[0].completed_on,
            None
        );
    }
}

#[tokio::test]
async fn an_ended_show_the_catalog_cannot_count_stays_watching() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = LibraryService::new(db.clone());
    let finished = Some(ReleaseStatus::Finished);

    // A season whose episode count is unknown.
    let unknown = show(db, 320, finished, Some(2), &[(1, Some(2)), (2, None)]).await;
    log(&svc, user.id, &unknown, &[(1, 1), (1, 2), (2, 1)], None).await;
    assert_eq!(status(&svc, user.id, &unknown).await, EntryStatus::Watching);

    // Fewer seasons stored than the show has.
    let missing = show(db, 321, finished, Some(3), &[(1, Some(2)), (2, Some(2))]).await;
    log(
        &svc,
        user.id,
        &missing,
        &[(1, 1), (1, 2), (2, 1), (2, 2)],
        None,
    )
    .await;
    assert_eq!(status(&svc, user.id, &missing).await, EntryStatus::Watching);

    // No season summary at all: logging creates season 1 without a count.
    let bare = show(db, 322, finished, None, &[]).await;
    log(&svc, user.id, &bare, &[(1, 1)], None).await;
    assert_eq!(status(&svc, user.id, &bare).await, EntryStatus::Watching);

    // Only specials stored.
    let specials = show(db, 323, finished, None, &[(0, Some(1))]).await;
    log(&svc, user.id, &specials, &[(0, 1)], None).await;
    assert_eq!(
        status(&svc, user.id, &specials).await,
        EntryStatus::Watching
    );
}

#[tokio::test]
async fn voided_plays_do_not_count_toward_finishing() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = ended(db, 330, ReleaseStatus::Finished).await;
    let svc = LibraryService::new(db.clone());

    log(&svc, user.id, &show, &[(1, 1), (1, 2), (2, 1)], None).await;
    svc.unwatch_episode(user.id, show.id, 1, 2)
        .await
        .unwrap()
        .unwrap();
    log(&svc, user.id, &show, &[(2, 2)], None).await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Watching);

    // Watching the voided episode again closes the last gap. The run
    // completes on the day the caller says it was watched (its own
    // calendar's date), not the UTC date of the instant, which is a day
    // earlier.
    svc.log_plays_on(
        user.id,
        &show,
        PlayBatch::Episodes(vec![(1, 2)]),
        Some(at("2024-03-31T23:00:00Z")),
        Some(RunDate {
            date: NaiveDate::from_ymd_opt(2024, 4, 1).unwrap(),
            precision: DatePrecision::Day,
        }),
        PlaySource::Manual,
    )
    .await
    .unwrap();
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
    let runs = svc.runs(user.id, show.id).await.unwrap();
    assert_eq!(runs[0].completed_on, NaiveDate::from_ymd_opt(2024, 4, 1));
    assert_eq!(runs[0].completed_precision, Some(DatePrecision::Day));
}

#[tokio::test]
async fn a_play_dated_before_its_run_started_moves_the_start() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = ended(db, 335, ReleaseStatus::Finished).await;
    let svc = LibraryService::new(db.clone());

    log(
        &svc,
        user.id,
        &show,
        &[(1, 1), (1, 2), (2, 1)],
        Some(at("2024-06-01T12:00:00Z")),
    )
    .await;
    // The run started today; the play that finishes it is dated earlier,
    // without a calendar date, so the run takes that instant's UTC date
    // for both ends rather than finishing before it started.
    log(
        &svc,
        user.id,
        &show,
        &[(2, 2)],
        Some(at("2024-05-20T12:00:00Z")),
    )
    .await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
    let runs = svc.runs(user.id, show.id).await.unwrap();
    assert_eq!(runs[0].started_on, NaiveDate::from_ymd_opt(2024, 5, 20));
    assert_eq!(runs[0].completed_on, NaiveDate::from_ymd_opt(2024, 5, 20));
}

#[tokio::test]
async fn a_movie_completes_on_its_plays_date_as_a_show_does() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let movie = common::movie(db, 338).await;
    let svc = LibraryService::new(db.clone());

    svc.log_plays(
        user.id,
        &movie,
        PlayBatch::Movie,
        Some(at("2024-02-10T12:00:00Z")),
        PlaySource::Manual,
        None,
    )
    .await
    .unwrap();
    assert_eq!(status(&svc, user.id, &movie).await, EntryStatus::Completed);
    let runs = svc.runs(user.id, movie.id).await.unwrap();
    assert_eq!(runs[0].started_on, NaiveDate::from_ymd_opt(2024, 2, 10));
    assert_eq!(runs[0].completed_on, NaiveDate::from_ymd_opt(2024, 2, 10));
}

#[tokio::test]
async fn re_logging_a_voided_episode_after_finishing_keeps_the_users_status() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = LibraryService::new(db.clone());
    for (n, chosen) in [(336, EntryStatus::Watching), (337, EntryStatus::Paused)] {
        let show = ended(db, n, ReleaseStatus::Finished).await;
        let all = [(1, 1), (1, 2), (2, 1), (2, 2)];
        log(&svc, user.id, &show, &all, None).await;
        assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
        let back = EntryPatch {
            status: Some(chosen),
            ..Default::default()
        };
        svc.patch(user.id, show.id, back).await.unwrap();

        // Un-ticking an episode voids its play and reopens the gap; ticking
        // it again closes it. The run already finished once, so the status
        // the user chose stands.
        let played = svc
            .unwatch_episode(user.id, show.id, 2, 2)
            .await
            .unwrap()
            .unwrap();
        assert!(played.voided_at.is_some());
        log(&svc, user.id, &show, &[(2, 2)], None).await;
        assert_eq!(status(&svc, user.id, &show).await, chosen, "{chosen:?}");

        // Nor when the user also cleared the run's completion date: the
        // run's events still record that it finished.
        let cleared = RunPatch {
            completed: Patch::Null,
            ..Default::default()
        };
        svc.update_run_by_number(user.id, show.id, 1, cleared)
            .await
            .unwrap();
        svc.unwatch_episode(user.id, show.id, 2, 2)
            .await
            .unwrap()
            .unwrap();
        log(&svc, user.id, &show, &[(2, 2)], None).await;
        assert_eq!(status(&svc, user.id, &show).await, chosen, "{chosen:?}");
        let runs = svc.runs(user.id, show.id).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].completed_on, None);
    }
}

#[tokio::test]
async fn a_status_the_user_chose_after_finishing_is_kept() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = ended(db, 340, ReleaseStatus::Finished).await;
    let svc = LibraryService::new(db.clone());

    log(
        &svc,
        user.id,
        &show,
        &[(1, 1), (1, 2), (2, 1), (2, 2)],
        None,
    )
    .await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
    let back = EntryPatch {
        status: Some(EntryStatus::Watching),
        ..Default::default()
    };
    svc.patch(user.id, show.id, back).await.unwrap();

    // Watching an episode again in the same, already finished run changes
    // nothing about the status the user set.
    log(&svc, user.id, &show, &[(1, 1)], None).await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Watching);
    svc.log_play(user.id, &show, LogPlay::manual(Some((0, 1))))
        .await
        .unwrap();
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Watching);

    // A rewatch is a new run, which its own last episode finishes.
    svc.start_rewatch(user.id, show.id, None).await.unwrap();
    log(&svc, user.id, &show, &[(1, 1), (1, 2), (2, 1)], None).await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Watching);
    log(&svc, user.id, &show, &[(2, 2)], None).await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
}

// ---------------------------------------------------------------- AniList

/// An AniList answering every query for a FINISHED show of `episodes`.
async fn anilist(episodes: i32) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            let id = body["variables"]["id"].as_i64().unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "Media": {
                "id": id,
                "title": {"romaji": "Mushishi", "english": "Mushi-Shi", "native": null},
                "description": "x", "startDate": {"year": 2005, "month": 10, "day": 23},
                "endDate": {"year": 2006, "month": 6, "day": 19},
                "coverImage": null, "bannerImage": null,
                "genres": ["Mystery"], "popularity": 1, "averageScore": 87,
                "episodes": episodes, "duration": 24, "status": "FINISHED", "format": "TV",
                "countryOfOrigin": "JP", "isAdult": false, "season": null,
                "seasonYear": null, "studios": {"nodes": []},
                "idMal": 457, "synonyms": [], "nextAiringEpisode": null,
            }}}))
        })
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_finished_anilist_show_completes_on_its_last_episode() {
    let t = test_db!();
    let db = &t.db;
    let tmdb = MockServer::start().await;
    let server = anilist(3).await;
    let settings = ProviderSettings {
        retry: RetrySettings {
            max_retries: 0,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(20),
        },
        tmdb_base_url: Some(tmdb.uri()),
        anilist_base_url: Some(server.uri()),
        ..ProviderSettings::default()
    };
    let provider: Arc<dyn MetadataProvider> = Arc::new(
        CameoProvider::new("0123456789abcdef0123456789abcdef", &settings).expect("adapter"),
    );
    let media = MediaService::new(db.clone(), provider);
    let show = media
        .ensure_in_catalog(&MediaId::new(Provider::AniList, MediaKind::Show, 457))
        .await
        .expect("stored");
    assert_eq!(show.status, Some(ReleaseStatus::Finished));

    let user = common::user(db).await;
    let svc = LibraryService::new(db.clone());
    log(&svc, user.id, &show, &[(1, 1), (1, 2)], None).await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Watching);
    log(
        &svc,
        user.id,
        &show,
        &[(1, 3)],
        Some(at("2025-01-10T23:00:00+00:00")),
    )
    .await;
    assert_eq!(status(&svc, user.id, &show).await, EntryStatus::Completed);
    let runs = svc.runs(user.id, show.id).await.unwrap();
    assert_eq!(runs[0].completed_on, NaiveDate::from_ymd_opt(2025, 1, 10));
}
