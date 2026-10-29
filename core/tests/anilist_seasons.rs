//! An AniList show's one synthesised season, through [`CameoProvider`]
//! against a wiremock AniList: its episode count from the show's total, from
//! the next airing episode, or unknown; every other season not found without
//! a request; and every failure of Stacks' episode-count query classified.
//! With a database, the season is stored in the catalog snapshot and keeps
//! answering which episodes exist while AniList is down, and a long season
//! costs a bounded number of statements, not one per episode, written all or
//! nothing.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sea_orm::{DatabaseConnection, Iterable, TransactionTrait};
use serde_json::{Value, json};
use stacks_core::entity::episode;
use stacks_core::library::episodes::season_episodes;
use stacks_core::media::catalog;
use stacks_core::media::models::{EpisodeInfo, SeasonInfo};
use stacks_core::media::provider::RetrySettings;
use stacks_core::media::{
    CameoProvider, MediaId, MediaKind, MetadataErrorKind, MetadataProvider, Provider,
    ProviderSettings,
};
use stacks_core::media::{MediaError, MediaService};
use wiremock::matchers::{any, body_string_contains, method};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const EPISODES_QUERY: &str = "StacksAnimeEpisodes";
const EXTRAS_QUERY: &str = "StacksMediaExtras";

fn adapter(anilist: &MockServer, tmdb: &MockServer) -> CameoProvider {
    let settings = ProviderSettings {
        retry: RetrySettings {
            max_retries: 0,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(20),
        },
        tmdb_base_url: Some(tmdb.uri()),
        anilist_base_url: Some(anilist.uri()),
        ..ProviderSettings::default()
    };
    CameoProvider::new("0123456789abcdef0123456789abcdef", &settings).expect("adapter builds")
}

fn show(numeric: i32) -> MediaId {
    MediaId::new(Provider::AniList, MediaKind::Show, numeric)
}

/// cameo's detail payload, and the reply to Stacks' own queries, for a show
/// with `episodes` in total and `next` as the next episode to air.
fn media(id: i64, episodes: Option<i32>, next: Option<i32>) -> Value {
    json!({ "data": { "Media": {
        "id": id,
        "title": {"romaji": "Sousou no Frieren", "english": "Frieren", "native": null},
        "description": "x", "startDate": {"year": 2023, "month": 9, "day": 29},
        "endDate": null, "coverImage": null, "bannerImage": null,
        "genres": ["Adventure"], "popularity": 1, "averageScore": 90,
        "episodes": episodes, "duration": 24, "status": "RELEASING", "format": "TV",
        "countryOfOrigin": "JP", "isAdult": false, "season": null,
        "seasonYear": null, "studios": {"nodes": []},
        "idMal": 52991, "synonyms": [],
        "nextAiringEpisode": next.map(|n| json!({ "episode": n })),
    }}})
}

/// An AniList answering every query for a show of `episodes` and `next`.
async fn anilist(episodes: Option<i32>, next: Option<i32>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            let id = body["variables"]["id"].as_i64().unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(media(id, episodes, next))
        })
        .mount(&server)
        .await;
    server
}

async fn requests_for(server: &MockServer, query: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| String::from_utf8_lossy(&r.body).contains(query))
        .count()
}

#[tokio::test]
async fn a_show_is_one_season_of_its_total_episodes() {
    let tmdb = MockServer::start().await;
    let server = anilist(Some(26), Some(30)).await;
    let p = adapter(&server, &tmdb);

    let title = p.details(&show(1)).await.expect("AniList answers");
    let seasons: Vec<(u32, Option<u32>)> = title
        .seasons
        .iter()
        .map(|s| (s.season_number, s.episode_count))
        .collect();
    assert_eq!(seasons, [(1, Some(26))], "the total wins over next airing");

    let season = p.season(&show(1), 1).await.expect("season 1");
    assert_eq!(season.season.season_number, 1);
    assert_eq!(season.season.episode_count, Some(26));
    let numbers: Vec<(u32, u32, Option<u32>)> = season
        .episodes
        .iter()
        .map(|e| (e.season_number, e.episode_number, e.absolute_number))
        .collect();
    let want: Vec<(u32, u32, Option<u32>)> = (1..=26).map(|n| (1, n, Some(n))).collect();
    assert_eq!(numbers, want);
    assert!(
        season
            .episodes
            .iter()
            .all(|e| e.name.is_none() && e.air_date.is_none())
    );
    assert_eq!(requests_for(&server, EPISODES_QUERY).await, 1);
    assert!(
        tmdb.received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

#[tokio::test]
async fn only_season_one_exists_and_nothing_is_asked_for_another() {
    let tmdb = MockServer::start().await;
    let server = anilist(Some(26), None).await;
    let p = adapter(&server, &tmdb);
    for number in [0, 2, 9] {
        let err = p
            .season(&show(1), number)
            .await
            .expect_err("no such season");
        assert_eq!(err.kind, MetadataErrorKind::NotFound, "season {number}");
    }
    let movie = MediaId::new(Provider::AniList, MediaKind::Movie, 1);
    let err = p.season(&movie, 1).await.expect_err("a movie has none");
    assert_eq!(err.kind, MetadataErrorKind::Unsupported);
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

#[tokio::test]
async fn an_airing_show_without_a_total_counts_to_the_next_episode() {
    let tmdb = MockServer::start().await;
    let server = anilist(None, Some(5)).await;
    let p = adapter(&server, &tmdb);

    let title = p.details(&show(2)).await.expect("AniList answers");
    assert_eq!(title.seasons.len(), 1);
    assert_eq!(title.seasons[0].episode_count, Some(4));
    let season = p.season(&show(2), 1).await.expect("season 1");
    assert_eq!(season.season.episode_count, Some(4));
    assert_eq!(
        season
            .episodes
            .iter()
            .map(|e| e.episode_number)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
}

#[tokio::test]
async fn an_unknown_count_is_an_empty_season() {
    let tmdb = MockServer::start().await;
    let server = anilist(None, None).await;
    let p = adapter(&server, &tmdb);

    let title = p.details(&show(3)).await.expect("AniList answers");
    assert_eq!(title.seasons.len(), 1);
    assert_eq!(title.seasons[0].episode_count, None);
    let season = p.season(&show(3), 1).await.expect("season 1");
    assert_eq!(season.season.episode_count, None);
    assert!(season.episodes.is_empty());
}

#[tokio::test]
async fn failing_extras_leave_the_season_to_cameos_count() {
    let tmdb = MockServer::start().await;
    let server = MockServer::start().await;
    Mock::given(body_string_contains(EXTRAS_QUERY))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(media(4, Some(12), None)))
        .mount(&server)
        .await;
    let p = adapter(&server, &tmdb);
    let title = p.details(&show(4)).await.expect("the title survives");
    assert_eq!(title.seasons.len(), 1);
    assert_eq!(title.seasons[0].episode_count, Some(12));
}

#[tokio::test]
async fn every_failure_of_the_episode_count_query_is_classified() {
    let raw = |status: u16, body: &str| {
        ResponseTemplate::new(status).set_body_raw(body.as_bytes().to_vec(), "application/json")
    };
    let cases = [
        (
            "a null Media",
            raw(200, r#"{"data":{"Media":null}}"#),
            "not_found",
        ),
        (
            "a GraphQL 404",
            raw(
                200,
                r#"{"data":{"Media":null},"errors":[{"message":"Not Found.","status":404}]}"#,
            ),
            "not_found",
        ),
        (
            "errors beside a null Media",
            raw(
                200,
                r#"{"data":{"Media":null},"errors":[{"message":"boom","status":500}]}"#,
            ),
            "bad_upstream_response",
        ),
        (
            "a 500 with a null Media",
            raw(500, r#"{"data":{"Media":null}}"#),
            "unavailable",
        ),
        (
            "a Media of nulls",
            raw(200, r#"{"data":{"Media":{"id":null,"episodes":null}}}"#),
            "bad_upstream_response",
        ),
        (
            "a count of the wrong type",
            raw(200, r#"{"data":{"Media":{"id":1,"episodes":"x"}}}"#),
            "bad_upstream_response",
        ),
        (
            "a 429",
            raw(429, "{}").insert_header("Retry-After", "7"),
            "rate_limited",
        ),
        ("a 503", raw(503, "<html>down</html>"), "unavailable"),
    ];
    for (name, response, class) in cases {
        let tmdb = MockServer::start().await;
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(response)
            .mount(&server)
            .await;
        let p = adapter(&server, &tmdb);
        let err = p.season(&show(5), 1).await.expect_err(name);
        assert_eq!(err.class(), class, "{name}: {err}");
        assert_eq!(err.provider, Provider::AniList, "{name}");
        assert_eq!(
            err.operation,
            stacks_core::media::Operation::Season,
            "{name}"
        );
    }
}

/// AniList failing every request from now on.
async fn take_down(server: &MockServer) {
    server.reset().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(server)
        .await;
}

#[tokio::test]
async fn the_season_is_stored_and_survives_an_outage() {
    let db = test_db!();
    let tmdb = MockServer::start().await;
    let server = anilist(Some(26), None).await;
    let provider: Arc<dyn MetadataProvider> = Arc::new(adapter(&server, &tmdb));
    let media = MediaService::new(db.db.clone(), provider);
    let id = show(6);

    // Tracking the show stores its season summary.
    let row = media.ensure_in_catalog(&id).await.expect("stored");
    let seasons = catalog::seasons_of(&db.db, row.id).await.expect("seasons");
    let stored: Vec<(i32, Option<i32>)> = seasons
        .iter()
        .map(|s| (s.number, s.episode_count))
        .collect();
    assert_eq!(stored, [(1, Some(26))]);

    // Asking which episodes exist reads the season and stores each episode,
    // numbered across the whole show.
    let want: Vec<i32> = (1..=26).collect();
    let numbers = season_episodes(&db.db, &media, &row, &id, 1)
        .await
        .expect("live");
    assert_eq!(numbers, Some(want.clone()));
    let episodes = catalog::episodes_of(&db.db, row.id, 1)
        .await
        .expect("episodes");
    assert_eq!(episodes.len(), 26);
    assert!(episodes.iter().all(|e| e.absolute_number == Some(e.number)));
    let none = season_episodes(&db.db, &media, &row, &id, 2)
        .await
        .expect("no season 2");
    assert_eq!(none, None);

    // AniList down: the stored season still answers, without a request.
    take_down(&server).await;
    let numbers = season_episodes(&db.db, &media, &row, &id, 1)
        .await
        .expect("stored");
    assert_eq!(numbers, Some(want));
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

#[tokio::test]
async fn a_stored_count_answers_when_the_first_season_read_fails() {
    let db = test_db!();
    let tmdb = MockServer::start().await;
    // Still airing with no total: eight episodes have aired.
    let server = anilist(None, Some(9)).await;
    let provider: Arc<dyn MetadataProvider> = Arc::new(adapter(&server, &tmdb));
    let media = MediaService::new(db.db.clone(), provider);
    let id = show(7);
    let row = media.ensure_in_catalog(&id).await.expect("stored");

    // Only the summary is stored when AniList fails the season read.
    take_down(&server).await;
    let numbers = season_episodes(&db.db, &media, &row, &id, 1)
        .await
        .expect("from the summary");
    assert_eq!(numbers, Some((1..=8).collect()));
    assert!(
        matches!(
            media.get_episodes(&id, 2).await,
            Err(MediaError::NotFound(_))
        ),
        "an AniList show has no season 2, whatever the outage"
    );
    let none = season_episodes(&db.db, &media, &row, &id, 2)
        .await
        .expect("no season 2");
    assert_eq!(none, None);
}

#[tokio::test]
async fn a_long_season_is_stored_in_a_bounded_number_of_statements() {
    let db = test_db!();
    let tmdb = MockServer::start().await;
    let server = anilist(Some(2_500), None).await;
    let provider: Arc<dyn MetadataProvider> = Arc::new(adapter(&server, &tmdb));
    let id = show(8);
    let row = MediaService::new(db.db.clone(), provider.clone())
        .ensure_in_catalog(&id)
        .await
        .expect("stored");

    // Every statement the season read sends, through its own connection.
    let statements = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut counted = db.db.clone();
    let log = statements.clone();
    counted.set_metric_callback(move |info| {
        if let Ok(mut log) = log.lock() {
            log.push(info.statement.sql.clone());
        }
    });
    let media = MediaService::new(counted, provider);
    let read = media.get_episodes(&id, 1).await.expect("live");
    assert_eq!(read.episodes.len(), 2_500);

    let sent = statements.lock().expect("log").clone();
    let inserts = sent
        .iter()
        .filter(|sql| sql.starts_with(r#"INSERT INTO "episodes""#))
        .count();
    assert_eq!(inserts, 2_500usize.div_ceil(catalog::EPISODE_BATCH));
    assert!(sent.len() <= 10, "{} statements: {sent:#?}", sent.len());
    let episodes = catalog::episodes_of(&db.db, row.id, 1).await.expect("rows");
    let numbers: Vec<i32> = episodes.iter().map(|e| e.number).collect();
    assert_eq!(numbers, (1..=2_500).collect::<Vec<_>>());

    // A full batch, every column bound, stays under PostgreSQL's limit.
    assert!(catalog::EPISODE_BATCH * episode::Column::iter().count() <= 65_535);
}

fn episode_info(number: u32, name: Option<&str>) -> EpisodeInfo {
    EpisodeInfo {
        season_number: 1,
        episode_number: number,
        absolute_number: None,
        name: name.map(str::to_owned),
        overview: None,
        air_date: None,
        runtime_minutes: None,
        still_url: None,
        rating: None,
    }
}

fn titles(rows: &[episode::Model]) -> Vec<(i32, Option<String>)> {
    let mut titles: Vec<_> = rows.iter().map(|e| (e.number, e.title.clone())).collect();
    titles.sort();
    titles
}

#[tokio::test]
async fn a_batch_keeps_one_listing_per_number_and_only_fills_in() {
    let db = test_db!();
    let row = common::show(&db.db, 9).await;
    let season = SeasonInfo {
        season_number: 1,
        name: None,
        overview: None,
        air_date: None,
        poster_url: None,
        episode_count: None,
    };
    let title = |n: i32, t: &str| (n, Some(t.to_owned()));

    // Episode 2 listed twice: one row, from its last listing.
    let first = [
        episode_info(1, Some("One")),
        episode_info(2, Some("Two")),
        episode_info(2, Some("Two, again")),
    ];
    let added = catalog::upsert_season_episodes(&db.db, row.id, &season, &first)
        .await
        .expect("stored");
    assert_eq!(titles(&added), [title(1, "One"), title(2, "Two, again")]);

    // A sparse refresh keeps what is known and reports only the new episode.
    let refresh = [episode_info(1, None), episode_info(3, Some("Three"))];
    let added = catalog::upsert_season_episodes(&db.db, row.id, &season, &refresh)
        .await
        .expect("refreshed");
    assert_eq!(titles(&added), [title(3, "Three")]);
    let stored = catalog::episodes_of(&db.db, row.id, 1).await.expect("rows");
    assert_eq!(
        titles(&stored),
        [title(1, "One"), title(2, "Two, again"), title(3, "Three")]
    );
}

/// The stored season names, after asserting episode 1 is all that is stored.
async fn untouched(db: &DatabaseConnection, media_id: uuid::Uuid) -> Vec<(i32, Option<String>)> {
    let episodes = catalog::episodes_of(db, media_id, 1).await.expect("rows");
    assert_eq!(titles(&episodes), [(1, Some("Kept".to_owned()))]);
    let seasons = catalog::seasons_of(db, media_id).await.expect("seasons");
    seasons
        .iter()
        .map(|s| (s.number, s.title.clone()))
        .collect()
}

#[tokio::test]
async fn a_failing_later_batch_leaves_the_season_untouched() {
    let db = test_db!();
    let row = common::show(&db.db, 10).await;
    let season = |name: &str| SeasonInfo {
        season_number: 1,
        name: Some(name.to_owned()),
        overview: None,
        air_date: None,
        poster_url: None,
        episode_count: None,
    };
    catalog::upsert_season_episodes(
        &db.db,
        row.id,
        &season("Old"),
        &[episode_info(1, Some("Kept"))],
    )
    .await
    .expect("stored");
    // The first batch is valid; PostgreSQL refuses the NUL in the second.
    let batch = u32::try_from(catalog::EPISODE_BATCH).expect("fits");
    let mut write: Vec<EpisodeInfo> = (1..=batch + 1)
        .map(|n| episode_info(n, Some("New")))
        .collect();
    write[usize::try_from(batch).expect("fits")].name = Some("bad\0".to_owned());

    // On its own connection, the whole write rolls back.
    catalog::upsert_season_episodes(&db.db, row.id, &season("New"), &write)
        .await
        .expect_err("the second batch fails");
    assert_eq!(
        untouched(&db.db, row.id).await,
        [(1, Some("Old".to_owned()))]
    );

    // Inside a caller's transaction, only its savepoint rolls back: the
    // caller's own earlier write survives and it still commits.
    let txn = db.db.begin().await.expect("begin");
    catalog::upsert_seasons(
        &txn,
        row.id,
        &[SeasonInfo {
            season_number: 2,
            ..season("Two")
        }],
    )
    .await
    .expect("caller's write");
    catalog::upsert_season_episodes(&txn, row.id, &season("New"), &write)
        .await
        .expect_err("the second batch fails");
    txn.commit().await.expect("the caller still commits");
    assert_eq!(
        untouched(&db.db, row.id).await,
        [(1, Some("Old".to_owned())), (2, Some("Two".to_owned()))]
    );
}
