//! AniList feeds through cameo: the airing feed (cameo's discover builder)
//! and AniList's community recommendations, against a wiremock AniList.
//! Their failures are covered by the fault sweep in
//! `metadata_fault_injection.rs`.

use std::time::Duration;

use serde_json::{Value, json};
use stacks_core::entity::enums::Category;
use stacks_core::media::breaker::BreakerConfig;
use stacks_core::media::provider::{RateSettings, RetrySettings};
use stacks_core::media::{
    CameoProvider, DiscoverCategory, MediaId, MediaKind, MetadataProvider, Provider,
    ProviderSettings,
};
use wiremock::matchers::{any, method};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn adapter(tmdb: &MockServer, anilist: &MockServer) -> CameoProvider {
    let unlimited = RateSettings {
        max_requests: 10_000,
        per: Duration::from_secs(1),
    };
    let settings = ProviderSettings {
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
        tmdb_base_url: Some(tmdb.uri()),
        anilist_base_url: Some(anilist.uri()),
        tmdb_language: None,
    };
    CameoProvider::new("0123456789abcdef0123456789abcdef", &settings).expect("adapter builds")
}

#[tokio::test]
async fn airing_anime_asks_anilist_for_releasing_titles() {
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("TMDB must not be called")
        .mount(&tmdb)
        .await;
    Mock::given(method("POST"))
        .respond_with(|req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            let query = body["query"].as_str().unwrap_or_default();
            let vars = &body["variables"];
            if !query.contains("DiscoverAnime")
                || vars["status"] != "RELEASING"
                || vars["sort"] != json!(["POPULARITY_DESC"])
            {
                return ResponseTemplate::new(400);
            }
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "Page": {
                "pageInfo": {"total": 60, "currentPage": 2, "lastPage": 3, "hasNextPage": true},
                "media": [
                    {
                        "id": 154587,
                        "title": {"romaji": "Sousou no Frieren", "english": "Frieren",
                                  "native": "葬送のフリーレン"},
                        "synonyms": ["Frieren: Beyond Journey's End"],
                        "description": "x", "startDate": {"year": 2023},
                        "coverImage": {"large": "l.jpg", "extraLarge": "xl.jpg"},
                        "bannerImage": "b.jpg", "genres": ["Adventure", "Sci-Fi"],
                        "averageScore": 90, "episodes": 28, "status": "RELEASING",
                        "countryOfOrigin": "JP", "format": vars["formatIn"][0]
                    },
                    {
                        "id": 21, "title": {"romaji": "ONE PIECE", "english": null,
                                            "native": null},
                        "description": null, "startDate": null, "coverImage": null,
                        "bannerImage": null, "genres": null, "averageScore": null,
                        "format": null
                    },
                    // Not a valid AniList id: cameo leaves it out.
                    {"id": 0, "title": null}
                ]
            }}}))
        })
        .mount(&anilist)
        .await;
    let p = adapter(&tmdb, &anilist);

    for (kind, format) in [(MediaKind::Show, "TV"), (MediaKind::Movie, "MOVIE")] {
        let page = p
            .feed(Provider::AniList, DiscoverCategory::Airing, kind, 2)
            .await
            .expect("AniList answers");
        let ids: Vec<&str> = page
            .items
            .iter()
            .map(|i| i.cameo_media_id.as_str())
            .collect();
        let want: Vec<String> = [154587, 21]
            .iter()
            .map(|n| MediaId::new(Provider::AniList, kind, *n).to_string())
            .collect();
        assert_eq!(ids, want, "{kind:?}");
        let frieren = &page.items[0];
        assert_eq!(frieren.title, "Frieren");
        assert_eq!(frieren.original_title.as_deref(), Some("葬送のフリーレン"));
        assert_eq!(frieren.year, Some(2023));
        assert_eq!(frieren.poster_url.as_deref(), Some("xl.jpg"));
        assert_eq!(frieren.rating, Some(9.0));
        assert_eq!(frieren.category, Category::Anime);
        assert_eq!(frieren.romaji_title.as_deref(), Some("Sousou no Frieren"));
        if kind == MediaKind::Show {
            assert_eq!(frieren.episode_count, Some(28), "lists carry the count");
        }
        assert_eq!(page.items[1].title, "ONE PIECE", "romaji when no English");
        assert!(page.unavailable.is_empty());
        assert_eq!(page.page, 2);
        assert!(page.has_next);
        let requests = anilist.received_requests().await.unwrap_or_default();
        let last: Value = serde_json::from_slice(&requests.last().expect("a request").body)
            .expect("a JSON request");
        assert_eq!(last["variables"]["page"], 2);
        assert_eq!(last["variables"]["formatIn"][0], format);
    }
    tmdb.verify().await;
}

/// AniList recommendations go to AniList: its community recommendations of
/// the title, of the asked-for kind.
#[tokio::test]
async fn anilist_recommendations_come_from_anilist() {
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("TMDB must not be called")
        .mount(&tmdb)
        .await;
    Mock::given(method("POST"))
        .respond_with(|req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            let query = body["query"].as_str().unwrap_or_default();
            if !query.contains("MediaRecommendations") || body["variables"]["id"] != 154587 {
                return ResponseTemplate::new(400);
            }
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "Media": {
                "recommendations": {
                    "pageInfo": {"total": 2, "currentPage": 1, "lastPage": 1,
                                 "hasNextPage": false},
                    "nodes": [
                        {"mediaRecommendation": {
                            "id": 52991, "title": {"romaji": "Kusuriya no Hitorigoto",
                                                   "english": "The Apothecary Diaries"},
                            "format": "TV", "episodes": 24
                        }},
                        {"mediaRecommendation": {
                            "id": 1535, "title": {"romaji": "Death Note"}, "format": "MOVIE"
                        }}
                    ]
                }
            }}}))
        })
        .mount(&anilist)
        .await;
    let p = adapter(&tmdb, &anilist);
    let show = MediaId::new(Provider::AniList, MediaKind::Show, 154587);
    let page = p.recommendations(&show, 1).await.expect("AniList answers");
    let ids: Vec<&str> = page
        .items
        .iter()
        .map(|i| i.cameo_media_id.as_str())
        .collect();
    assert_eq!(ids, ["anilist:show:52991"], "the shows among them");
    assert_eq!(page.items[0].title, "The Apothecary Diaries");
    assert_eq!(page.items[0].episode_count, Some(24));
    assert!(!page.has_next);
    tmdb.verify().await;
}
