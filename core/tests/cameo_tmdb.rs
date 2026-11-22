//! TMDB through cameo, as the adapter sees it.
//!
//! cameo generates its TMDB client, which decides what reaches the wire and
//! what a sparse reply decodes to. These pin what Stacks relies on: the
//! credential's transport, the provider's own status words, the one page
//! TMDB trending has, and what the adapter makes of a reply of `null`s.

use std::time::Duration;

use serde_json::{Value, json};
use stacks_core::media::breaker::BreakerConfig;
use stacks_core::media::provider::{RateSettings, RetrySettings};
use stacks_core::media::{
    CameoProvider, DiscoverCategory, MediaId, MediaKind, MetadataProvider, Provider,
    ProviderSettings,
};
use wiremock::matchers::{header, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A v3 API key: 32 hex characters.
const V3_KEY: &str = "0123456789abcdef0123456789abcdef";
/// A v4 read access token: a JWT, so it has dots.
const V4_TOKEN: &str = "eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJzdGFja3MifQ.c2lnbmF0dXJl";

fn settings(tmdb: &str) -> ProviderSettings {
    let unlimited = RateSettings {
        max_requests: 10_000,
        per: Duration::from_secs(1),
    };
    ProviderSettings {
        connect_timeout: Duration::from_millis(300),
        request_timeout: Duration::from_millis(500),
        call_deadline: Duration::from_secs(5),
        retry: RetrySettings {
            max_retries: 0,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(20),
        },
        tmdb_rate: unlimited,
        anilist_rate: unlimited,
        anilist_extras_rate: unlimited,
        breaker: BreakerConfig {
            failure_threshold: 1_000,
            cooldown: Duration::from_secs(60),
        },
        tmdb_base_url: Some(tmdb.to_owned()),
        // Nothing here reaches AniList.
        anilist_base_url: Some("http://127.0.0.1:9".to_owned()),
        tmdb_language: None,
    }
}

fn adapter(credential: &str, tmdb: &MockServer) -> CameoProvider {
    CameoProvider::new(credential, &settings(&tmdb.uri())).expect("adapter builds")
}

fn fight_club() -> Value {
    json!({
        "id": 550, "title": "Fight Club", "original_title": "Fight Club",
        "original_language": "en", "overview": "x", "release_date": "1999-10-15",
        "genres": [{"id": 18, "name": "Drama"}], "runtime": 139, "status": "Released",
        "imdb_id": "tt0137523", "production_countries": [], "vote_average": 8.4
    })
}

fn movie() -> MediaId {
    MediaId::new(Provider::Tmdb, MediaKind::Movie, 550)
}

#[tokio::test]
async fn a_v3_key_travels_as_the_api_key_parameter() {
    let tmdb = MockServer::start().await;
    Mock::given(path("/3/movie/550"))
        .and(query_param("api_key", V3_KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(fight_club()))
        .expect(1)
        .mount(&tmdb)
        .await;
    let title = adapter(V3_KEY, &tmdb).details(&movie()).await.expect("ok");
    assert_eq!(title.detail.media.title, "Fight Club");
}

#[tokio::test]
async fn a_v4_token_travels_as_a_bearer_header() {
    let tmdb = MockServer::start().await;
    Mock::given(path("/3/movie/550"))
        .and(header(
            "authorization",
            format!("Bearer {V4_TOKEN}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fight_club()))
        .expect(1)
        .mount(&tmdb)
        .await;
    let title = adapter(V4_TOKEN, &tmdb)
        .details(&movie())
        .await
        .expect("ok");
    assert_eq!(title.detail.media.title, "Fight Club");
    let requests = tmdb.received_requests().await.expect("recorded");
    assert!(requests.iter().all(|r| !r.url.as_str().contains("api_key")));
}

#[tokio::test]
async fn tmdb_in_production_comes_from_the_status() {
    let tmdb = MockServer::start().await;
    // `in_production` is missing (cameo reads it as unknown) but the show is
    // still returning.
    Mock::given(path("/3/tv/1396"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1396, "name": "Show", "original_name": "Show", "overview": "x",
            "first_air_date": "2008-01-20", "genres": [], "origin_country": ["US"],
            "original_language": "en", "status": "Returning Series", "type": "Miniseries",
            "number_of_seasons": 2, "number_of_episodes": 20, "episode_run_time": [45],
            "seasons": [
                {"season_number": 1, "name": "Season 1", "episode_count": 10,
                 "air_date": "2008-01-20", "overview": "", "poster_path": null, "id": 1},
                {"season_number": 2, "name": "Season 2", "episode_count": 10,
                 "air_date": "2009-01-20", "overview": "", "poster_path": null, "id": 2}
            ]
        })))
        .mount(&tmdb)
        .await;
    let title = adapter(V3_KEY, &tmdb)
        .details(&MediaId::new(Provider::Tmdb, MediaKind::Show, 1396))
        .await
        .expect("ok");
    assert_eq!(title.detail.in_production, Some(true));
    // cameo parses the status and type; Stacks publishes TMDB's own words.
    assert_eq!(title.detail.status.as_deref(), Some("Returning Series"));
    assert_eq!(
        title.detail.format,
        Some(stacks_core::entity::enums::MediaFormat::Miniseries)
    );
    assert_eq!(title.seasons.len(), 2);
    assert_eq!(title.seasons[1].episode_count, Some(10));
}

#[tokio::test]
async fn tmdb_in_production_ignores_the_flag_between_seasons() {
    let tmdb = MockServer::start().await;
    // A returning show between seasons: TMDB's flag says production has
    // stopped, but the show has not ended, and a stale snapshot (which has
    // only the status) would say it is still being made.
    Mock::given(path("/3/tv/1396"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1396, "name": "Show", "status": "Returning Series",
            "in_production": false, "seasons": []
        })))
        .mount(&tmdb)
        .await;
    let title = adapter(V3_KEY, &tmdb)
        .details(&MediaId::new(Provider::Tmdb, MediaKind::Show, 1396))
        .await
        .expect("ok");
    assert_eq!(title.detail.in_production, Some(true));
}

#[tokio::test]
async fn tmdb_trending_is_one_page() {
    let tmdb = MockServer::start().await;
    let mut item = fight_club();
    item["genre_ids"] = json!([18]);
    Mock::given(path("/3/trending/movie/week"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "page": 1, "results": [item], "total_pages": 500, "total_results": 10_000
        })))
        .expect(1)
        .mount(&tmdb)
        .await;
    let p = adapter(V3_KEY, &tmdb);
    let first = p
        .feed(
            Provider::Tmdb,
            DiscoverCategory::Trending,
            MediaKind::Movie,
            1,
        )
        .await
        .expect("ok");
    assert_eq!(first.items.len(), 1);
    // TMDB says 500 pages, but every one is the first: none follows.
    assert!(!first.has_next);
    // A later page (an old cursor) is empty, and asks TMDB nothing.
    let second = p
        .feed(
            Provider::Tmdb,
            DiscoverCategory::Trending,
            MediaKind::Movie,
            2,
        )
        .await
        .expect("ok");
    assert!(second.items.is_empty());
    assert!(!second.has_next);
}

#[tokio::test]
async fn a_tmdb_list_of_nulls_reads_as_empty() {
    let tmdb = MockServer::start().await;
    Mock::given(path("/3/search/multi"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"page": null, "results": null, "total_pages": null})),
        )
        .mount(&tmdb)
        .await;
    let page = adapter(V3_KEY, &tmdb)
        .search(Provider::Tmdb, "fight", 1)
        .await
        .expect("cameo reads it as an empty page");
    assert!(page.items.is_empty());
    assert!(!page.has_next);
}

#[tokio::test]
async fn tmdb_details_naming_no_title_are_refused() {
    let tmdb = MockServer::start().await;
    Mock::given(path("/3/movie/550"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": null, "title": null})))
        .mount(&tmdb)
        .await;
    Mock::given(path("/3/tv/1396"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 1396, "name": " "})))
        .mount(&tmdb)
        .await;
    let p = adapter(V3_KEY, &tmdb);
    let show = MediaId::new(Provider::Tmdb, MediaKind::Show, 1396);
    for id in [movie(), show] {
        let err = p.details(&id).await.expect_err("no title");
        assert_eq!(err.class(), "bad_upstream_response", "{id}: {err}");
    }
}
