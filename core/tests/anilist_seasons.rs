//! An AniList show's one synthesised season, through [`CameoProvider`]
//! against a wiremock AniList: its episode count from the show's total, from
//! the next airing episode, or unknown; every other season not found without
//! a request; and every failure of Stacks' episode-count query classified.

use std::time::Duration;

use serde_json::{Value, json};
use stacks_core::media::provider::RetrySettings;
use stacks_core::media::{
    CameoProvider, MediaId, MediaKind, MetadataErrorKind, MetadataProvider, Provider,
    ProviderSettings,
};
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
