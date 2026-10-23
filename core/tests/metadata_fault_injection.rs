//! Fault injection for the metadata adapter.
//!
//! [`CameoProvider`] is pointed at wiremock servers standing in for TMDB and
//! AniList, which then misbehave in every way the issue lists: 5xx, 429 with
//! and without `Retry-After`, connect and body timeouts, explicit `null`s,
//! truncated and empty bodies, and GraphQL errors on a 200. Every adapter
//! operation must come back with the right class, count it in
//! `stacks_metadata_errors_total`, and never panic. Separate tests cover the
//! circuit breaker, retries, the rate limit, provider routing, and the two
//! source-level rules: only the adapter names cameo, and no metadata result
//! is swallowed with `.ok()` / `unwrap*`.
//!
//! The tests share one global metrics recorder, so they hold [`SERIAL`] to
//! keep each test's counter deltas its own.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};
use metrics_util::{CompositeKey, MetricKind};
use serde_json::{Value, json};
use stacks_core::media::breaker::{BreakerConfig, BreakerState};
use stacks_core::media::provider::{RateSettings, RetrySettings};
use stacks_core::media::{
    CameoProvider, DiscoverCategory, MediaId, MediaKind, MetadataError, MetadataErrorKind,
    MetadataProvider, Operation, Provider, ProviderSettings, UnavailableCause,
};
use tokio::sync::Mutex;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

static SERIAL: Mutex<()> = Mutex::const_new(());

fn snapshotter() -> &'static Snapshotter {
    static SNAP: OnceLock<Snapshotter> = OnceLock::new();
    SNAP.get_or_init(|| {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        recorder
            .install()
            .expect("no other recorder in this test binary");
        snapshotter
    })
}

/// Every metric value since the previous call (counters and gauges reset on
/// read).
fn drain() -> Vec<(CompositeKey, DebugValue)> {
    snapshotter()
        .snapshot()
        .into_vec()
        .into_iter()
        .map(|(k, _, _, v)| (k, v))
        .collect()
}

fn labels_match(key: &CompositeKey, want: &[(&str, &str)]) -> bool {
    want.iter().all(|(name, value)| {
        key.key()
            .labels()
            .any(|l| l.key() == *name && l.value() == *value)
    })
}

fn error_count(metrics: &[(CompositeKey, DebugValue)], labels: &[(&str, &str)]) -> u64 {
    metrics
        .iter()
        .filter(|(k, _)| {
            k.kind() == MetricKind::Counter
                && k.key().name() == "stacks_metadata_errors_total"
                && labels_match(k, labels)
        })
        .map(|(_, v)| match v {
            DebugValue::Counter(n) => *n,
            _ => 0,
        })
        .sum()
}

fn timed(metrics: &[(CompositeKey, DebugValue)], labels: &[(&str, &str)]) -> usize {
    metrics
        .iter()
        .filter(|(k, _)| {
            k.kind() == MetricKind::Histogram
                && k.key().name() == "stacks_metadata_request_duration_seconds"
                && labels_match(k, labels)
        })
        .map(|(_, v)| match v {
            DebugValue::Histogram(xs) => xs.len(),
            _ => 0,
        })
        .sum()
}

fn circuit_gauge(metrics: &[(CompositeKey, DebugValue)], provider: &str) -> Option<f64> {
    metrics.iter().find_map(|(k, v)| match v {
        DebugValue::Gauge(g)
            if k.key().name() == "stacks_metadata_circuit_state"
                && labels_match(k, &[("provider", provider)]) =>
        {
            Some(g.into_inner())
        }
        _ => None,
    })
}

/// Settings for tests: fast timeouts, no retries, effectively no rate limit,
/// and a breaker that the fault matrix never trips.
fn settings(tmdb: &str, anilist: &str) -> ProviderSettings {
    ProviderSettings {
        connect_timeout: Duration::from_millis(300),
        request_timeout: Duration::from_millis(500),
        call_deadline: Duration::from_secs(5),
        retry: RetrySettings {
            max_retries: 0,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(20),
        },
        tmdb_rate: RateSettings {
            max_requests: 10_000,
            per: Duration::from_secs(1),
        },
        anilist_rate: RateSettings {
            max_requests: 10_000,
            per: Duration::from_secs(1),
        },
        breaker: BreakerConfig {
            failure_threshold: 1_000,
            cooldown: Duration::from_secs(60),
        },
        tmdb_base_url: Some(tmdb.to_owned()),
        anilist_base_url: Some(anilist.to_owned()),
        tmdb_language: None,
    }
}

fn adapter(s: &ProviderSettings) -> CameoProvider {
    CameoProvider::new("0123456789abcdef0123456789abcdef", s).expect("adapter builds")
}

/// One adapter operation, with ids of the provider it targets.
#[derive(Debug, Clone, Copy)]
enum Op {
    Search(Provider),
    Details(Provider, MediaKind),
    Season,
    Recommendations(MediaKind),
    Feed(Provider, DiscoverCategory, MediaKind),
    MalLookup,
}

impl Op {
    fn provider(self) -> Provider {
        match self {
            Op::Search(p) | Op::Details(p, _) | Op::Feed(p, _, _) => p,
            Op::Season | Op::Recommendations(_) => Provider::Tmdb,
            Op::MalLookup => Provider::AniList,
        }
    }

    fn operation(self) -> Operation {
        match self {
            Op::Search(_) => Operation::Search,
            Op::Details(..) => Operation::Details,
            Op::Season => Operation::Season,
            Op::Recommendations(_) => Operation::Recommendations,
            Op::Feed(..) => Operation::Feed,
            Op::MalLookup => Operation::Extras,
        }
    }

    /// Every operation the adapter sends to a provider.
    fn all() -> Vec<Op> {
        let mut ops = Vec::new();
        for p in [Provider::Tmdb, Provider::AniList] {
            ops.push(Op::Search(p));
            for k in [MediaKind::Movie, MediaKind::Show] {
                ops.push(Op::Details(p, k));
                for f in [
                    DiscoverCategory::Trending,
                    DiscoverCategory::Popular,
                    DiscoverCategory::TopRated,
                ] {
                    ops.push(Op::Feed(p, f, k));
                }
            }
        }
        ops.push(Op::Season);
        ops.push(Op::Recommendations(MediaKind::Movie));
        ops.push(Op::Recommendations(MediaKind::Show));
        ops.push(Op::MalLookup);
        ops
    }

    async fn run(self, p: &CameoProvider) -> Result<(), MetadataError> {
        let id = |provider, kind| MediaId::new(provider, kind, 21);
        match self {
            Op::Search(provider) => p.search(provider, "frieren", 1).await.map(drop),
            Op::Details(provider, kind) => p.details(&id(provider, kind)).await.map(drop),
            Op::Season => p
                .season(&id(Provider::Tmdb, MediaKind::Show), 1)
                .await
                .map(drop),
            Op::Recommendations(kind) => p
                .recommendations(&id(Provider::Tmdb, kind), 1)
                .await
                .map(drop),
            Op::Feed(provider, feed, kind) => p.feed(provider, feed, kind, 1).await.map(drop),
            Op::MalLookup => p.anilist_by_mal_id(21).await.map(drop),
        }
    }
}

/// A fault the mock upstream injects, and the class it must produce.
struct Fault {
    name: &'static str,
    response: ResponseTemplate,
    class: &'static str,
    /// Only meaningful for one provider (GraphQL errors are AniList's).
    only: Option<Provider>,
}

fn faults() -> Vec<Fault> {
    let body = |status: u16, raw: &str| {
        ResponseTemplate::new(status).set_body_raw(raw.as_bytes().to_vec(), "application/json")
    };
    vec![
        Fault {
            name: "500",
            response: body(
                500,
                r#"{"status_code":11,"status_message":"Internal error"}"#,
            ),
            class: "unavailable",
            only: None,
        },
        Fault {
            name: "503",
            response: body(503, "<html>Service Unavailable</html>"),
            class: "unavailable",
            only: None,
        },
        Fault {
            name: "429 without Retry-After",
            response: body(429, r#"{"status_code":25,"status_message":"Too many"}"#),
            class: "rate_limited",
            only: None,
        },
        Fault {
            name: "429 with Retry-After",
            response: body(429, r#"{"status_code":25,"status_message":"Too many"}"#)
                .insert_header("Retry-After", "7"),
            class: "rate_limited",
            only: None,
        },
        Fault {
            name: "slow body past the request timeout",
            response: body(200, "{}").set_delay(Duration::from_secs(3)),
            class: "unavailable",
            only: None,
        },
        Fault {
            name: "explicit null in a required field",
            response: body(
                200,
                r#"{"id":null,"page":null,"results":null,"title":null,"name":null,
                    "episodes":null,"season_number":null,
                    "data":{"Media":{"id":null},"Page":{"pageInfo":null,"media":[{"id":null}]}}}"#,
            ),
            class: "bad_upstream_response",
            only: None,
        },
        Fault {
            name: "truncated JSON",
            response: body(200, r#"{"page": 1, "results": [{"id": 5"#),
            class: "bad_upstream_response",
            only: None,
        },
        Fault {
            name: "empty 200 body",
            response: body(200, ""),
            class: "bad_upstream_response",
            only: None,
        },
        Fault {
            name: "GraphQL errors on a 200",
            response: body(
                200,
                r#"{"data":null,"errors":[{"message":"Internal Server Error","status":500}]}"#,
            ),
            class: "bad_upstream_response",
            only: Some(Provider::AniList),
        },
    ]
}

#[tokio::test]
async fn every_operation_classifies_every_fault() {
    let _serial = SERIAL.lock().await;
    let mut failures = Vec::new();
    for fault in faults() {
        let tmdb = MockServer::start().await;
        let anilist = MockServer::start().await;
        Mock::given(any())
            .respond_with(fault.response.clone())
            .mount(&tmdb)
            .await;
        Mock::given(any())
            .respond_with(fault.response.clone())
            .mount(&anilist)
            .await;
        let p = adapter(&settings(&tmdb.uri(), &anilist.uri()));

        for op in Op::all() {
            if fault.only.is_some_and(|only| only != op.provider()) {
                continue;
            }
            drain();
            let started = Instant::now();
            let result = op.run(&p).await;
            let metrics = drain();
            let labels = [
                ("provider", op.provider().as_str()),
                ("operation", op.operation().as_str()),
                ("class", fault.class),
            ];
            match result {
                Ok(()) => failures.push(format!("{op:?} / {}: succeeded", fault.name)),
                Err(e) if e.class() != fault.class => failures.push(format!(
                    "{op:?} / {}: got {} ({e}), want {}",
                    fault.name,
                    e.class(),
                    fault.class
                )),
                Err(e) => {
                    if e.provider != op.provider() || e.operation != op.operation() {
                        failures.push(format!("{op:?} / {}: mislabelled {e}", fault.name));
                    }
                    if error_count(&metrics, &labels) != 1 {
                        failures.push(format!(
                            "{op:?} / {}: error metric not counted once",
                            fault.name
                        ));
                    }
                    let outcome = [
                        ("provider", op.provider().as_str()),
                        ("operation", op.operation().as_str()),
                        ("outcome", fault.class),
                    ];
                    if timed(&metrics, &outcome) != 1 {
                        failures.push(format!("{op:?} / {}: latency not recorded", fault.name));
                    }
                    if fault.name == "429 with Retry-After"
                        && e.retry_after() != Some(Duration::from_secs(7))
                    {
                        failures.push(format!(
                            "{op:?} / {}: Retry-After lost ({:?})",
                            fault.name,
                            e.retry_after()
                        ));
                    }
                }
            }
            if started.elapsed() > Duration::from_secs(4) {
                failures.push(format!("{op:?} / {}: not bounded", fault.name));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn a_connect_timeout_is_unavailable_and_bounded() {
    let _serial = SERIAL.lock().await;
    // A non-routable address: the connection attempt hangs (or is refused
    // outright by the sandbox), and either way must come back classified.
    let s = settings("http://10.255.255.1:81", "http://10.255.255.1:81");
    let p = adapter(&s);
    for op in Op::all() {
        let started = Instant::now();
        let err = op.run(&p).await.expect_err("nothing is listening");
        assert!(
            matches!(
                err.kind,
                MetadataErrorKind::Unavailable(
                    UnavailableCause::Transport(_) | UnavailableCause::Timeout
                )
            ),
            "{op:?}: {err}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{op:?} not bounded"
        );
    }
}

/// Minimal TMDB movie detail payload.
fn tmdb_movie() -> Value {
    json!({
        "adult": false, "backdrop_path": null, "belongs_to_collection": null,
        "budget": 0, "genres": [{"id": 18, "name": "Drama"}], "homepage": "",
        "id": 550, "imdb_id": "tt0137523", "original_language": "en",
        "original_title": "Fight Club", "overview": "x", "popularity": 1.0,
        "poster_path": null, "production_companies": [], "production_countries": [],
        "release_date": "1999-10-15", "revenue": 0, "runtime": 139,
        "spoken_languages": [], "status": "Released", "tagline": "t",
        "title": "Fight Club", "video": false, "vote_average": 8.4, "vote_count": 1
    })
}

/// Minimal AniList detail payload.
fn anilist_media(id: i64) -> Value {
    json!({ "data": { "Media": {
        "id": id,
        "title": {"romaji": "Sousou no Frieren", "english": "Frieren", "native": "葬送のフリーレン"},
        "description": "x", "startDate": {"year": 2023, "month": 9, "day": 29},
        "endDate": null, "coverImage": null, "bannerImage": null,
        "genres": ["Adventure"], "popularity": 1, "averageScore": 90,
        "episodes": 28, "duration": 24, "status": "FINISHED", "format": "TV",
        "countryOfOrigin": "JP", "isAdult": false, "season": null,
        "seasonYear": null, "studios": {"nodes": []},
        "idMal": 52991, "synonyms": ["Frieren: Beyond Journey's End"]
    }}})
}

#[tokio::test]
async fn the_breaker_opens_after_the_threshold_and_recovers() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    let mut s = settings(&tmdb.uri(), &anilist.uri());
    s.breaker = BreakerConfig {
        failure_threshold: 3,
        cooldown: Duration::from_millis(300),
    };
    let p = adapter(&s);
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 550);

    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .expect(3)
        .named("failing TMDB")
        .mount(&tmdb)
        .await;
    for _ in 0..3 {
        let err = p.details(&id).await.expect_err("503");
        assert_eq!(
            err.kind,
            MetadataErrorKind::Unavailable(UnavailableCause::Status(503))
        );
    }
    assert_eq!(p.circuit_state(Provider::Tmdb), BreakerState::Open);
    assert_eq!(circuit_gauge(&drain(), "tmdb"), Some(2.0));

    // Open: fails fast without a request (the mock expects exactly three).
    let err = p.details(&id).await.expect_err("circuit open");
    assert_eq!(
        err.kind,
        MetadataErrorKind::Unavailable(UnavailableCause::CircuitOpen)
    );
    // AniList has its own breaker and is unaffected.
    assert_eq!(p.circuit_state(Provider::AniList), BreakerState::Closed);
    tmdb.verify().await;

    // After the cooldown one probe goes out; it succeeds and closes the
    // circuit.
    tmdb.reset().await;
    Mock::given(method("GET"))
        .and(path("/3/movie/550"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tmdb_movie()))
        .mount(&tmdb)
        .await;
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(p.circuit_state(Provider::Tmdb), BreakerState::HalfOpen);
    let title = p.details(&id).await.expect("the probe succeeds");
    assert_eq!(title.snapshot.title, "Fight Club");
    assert_eq!(p.circuit_state(Provider::Tmdb), BreakerState::Closed);
    assert_eq!(circuit_gauge(&drain(), "tmdb"), Some(0.0));
}

#[tokio::test]
async fn a_failed_probe_reopens_the_breaker() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    let mut s = settings(&tmdb.uri(), &anilist.uri());
    s.breaker = BreakerConfig {
        failure_threshold: 1,
        cooldown: Duration::from_millis(200),
    };
    let p = adapter(&s);
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .mount(&tmdb)
        .await;
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 1);
    assert!(p.details(&id).await.is_err());
    assert_eq!(p.circuit_state(Provider::Tmdb), BreakerState::Open);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let err = p.details(&id).await.expect_err("probe fails");
    assert_eq!(
        err.kind,
        MetadataErrorKind::Unavailable(UnavailableCause::Status(500))
    );
    assert_eq!(p.circuit_state(Provider::Tmdb), BreakerState::Open);
}

#[tokio::test]
async fn retries_cover_5xx_and_429_but_not_other_4xx() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    let mut s = settings(&tmdb.uri(), &anilist.uri());
    s.retry.max_retries = 1;
    let p = adapter(&s);

    // A 5xx is retried once, then classified.
    Mock::given(path("/3/movie/1"))
        .respond_with(ResponseTemplate::new(502))
        .expect(2)
        .mount(&tmdb)
        .await;
    let err = p
        .details(&MediaId::new(Provider::Tmdb, MediaKind::Movie, 1))
        .await
        .expect_err("502 twice");
    assert_eq!(err.class(), "unavailable");

    // A 401 is the operator's problem: one request, no retry.
    Mock::given(path("/3/movie/2"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"status_code": 7, "status_message": "Invalid API key"})),
        )
        .expect(1)
        .mount(&tmdb)
        .await;
    let err = p
        .details(&MediaId::new(Provider::Tmdb, MediaKind::Movie, 2))
        .await
        .expect_err("401");
    assert_eq!(err.class(), "misconfigured");

    // A 422 is not retried either.
    Mock::given(path("/3/movie/3"))
        .respond_with(ResponseTemplate::new(422))
        .expect(1)
        .mount(&tmdb)
        .await;
    let err = p
        .details(&MediaId::new(Provider::Tmdb, MediaKind::Movie, 3))
        .await
        .expect_err("422");
    assert_eq!(err.class(), "misconfigured");

    // A 429 honours Retry-After, then the retry succeeds.
    Mock::given(path("/3/movie/550"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&tmdb)
        .await;
    Mock::given(path("/3/movie/550"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tmdb_movie()))
        .expect(1)
        .mount(&tmdb)
        .await;
    let started = Instant::now();
    p.details(&MediaId::new(Provider::Tmdb, MediaKind::Movie, 550))
        .await
        .expect("retried after the 429");
    assert!(started.elapsed() >= Duration::from_secs(1));
    tmdb.verify().await;
}

#[tokio::test]
async fn the_rate_limit_paces_requests() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    let mut s = settings(&tmdb.uri(), &anilist.uri());
    s.tmdb_rate = RateSettings {
        max_requests: 2,
        per: Duration::from_secs(1),
    };
    let p = adapter(&s);
    Mock::given(path("/3/movie/550"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tmdb_movie()))
        .mount(&tmdb)
        .await;
    let id = MediaId::new(Provider::Tmdb, MediaKind::Movie, 550);
    let started = Instant::now();
    for _ in 0..4 {
        p.details(&id).await.expect("ok");
    }
    // Two in the burst, then one every 500ms.
    assert!(
        started.elapsed() >= Duration::from_millis(900),
        "4 requests at 2/s took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn anilist_ids_never_reach_tmdb() {
    let _serial = SERIAL.lock().await;
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
            let id = body["variables"]["id"].as_i64().unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(anilist_media(id))
        })
        .mount(&anilist)
        .await;
    let p = adapter(&settings(&tmdb.uri(), &anilist.uri()));

    for kind in [MediaKind::Movie, MediaKind::Show] {
        let id = MediaId::new(Provider::AniList, kind, 154587);
        let title = p.details(&id).await.expect("AniList answers");
        assert_eq!(title.snapshot.id, id);
        let err = p.recommendations(&id, 1).await.expect_err("unsupported");
        assert_eq!(err.kind, MetadataErrorKind::Unsupported);
    }
    let show = MediaId::new(Provider::AniList, MediaKind::Show, 154587);
    let err = p.season(&show, 1).await.expect_err("unsupported");
    assert_eq!(err.kind, MetadataErrorKind::Unsupported);
    // Search and feeds name their provider; AniList's go to AniList (whose
    // mock answers them with a detail payload, so they come back unusable).
    for op in Op::all()
        .into_iter()
        .filter(|op| op.provider() == Provider::AniList)
    {
        if let Err(e) = op.run(&p).await {
            assert_eq!(e.provider, Provider::AniList, "{op:?}");
        }
    }
    tmdb.verify().await;
}

#[tokio::test]
async fn tmdb_ids_never_reach_anilist() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("AniList must not be called")
        .mount(&anilist)
        .await;
    Mock::given(path("/3/movie/550"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tmdb_movie()))
        .mount(&tmdb)
        .await;
    let p = adapter(&settings(&tmdb.uri(), &anilist.uri()));
    let title = p
        .details(&MediaId::new(Provider::Tmdb, MediaKind::Movie, 550))
        .await
        .expect("TMDB answers");
    assert_eq!(title.detail.imdb_id.as_deref(), Some("tt0137523"));
    anilist.verify().await;
}

#[tokio::test]
async fn anilist_details_capture_romaji_mal_and_language() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            let id = body["variables"]["id"].as_i64().unwrap_or(0);
            ResponseTemplate::new(200).set_body_json(anilist_media(id))
        })
        .expect(2) // cameo's detail query, then Stacks' extras query
        .mount(&anilist)
        .await;
    let p = adapter(&settings(&tmdb.uri(), &anilist.uri()));
    let title = p
        .details(&MediaId::new(Provider::AniList, MediaKind::Show, 154587))
        .await
        .expect("ok");
    let snap = &title.snapshot;
    assert_eq!(snap.romaji_title.as_deref(), Some("Sousou no Frieren"));
    assert!(
        snap.extra_refs
            .iter()
            .any(|(p, v)| p.as_str() == "mal" && v == "52991")
    );
    assert_eq!(snap.original_language.as_deref(), Some("ja"));
    assert_eq!(snap.episode_count, Some(28));
    assert_eq!(title.detail.in_production, Some(false));
    anilist.verify().await;
}

#[tokio::test]
async fn failing_extras_do_not_fail_the_title() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    // cameo's detail query succeeds; Stacks' own query gets a 503.
    Mock::given(method("POST"))
        .respond_with(|req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            if body["query"]
                .as_str()
                .is_some_and(|q| q.contains("StacksMediaExtras"))
            {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(anilist_media(1535))
            }
        })
        .mount(&anilist)
        .await;
    let p = adapter(&settings(&tmdb.uri(), &anilist.uri()));
    drain();
    let title = p
        .details(&MediaId::new(Provider::AniList, MediaKind::Movie, 1535))
        .await
        .expect("the title survives");
    assert_eq!(title.snapshot.romaji_title, None);
    // Language still comes from Stacks' table, not cameo's.
    assert_eq!(title.snapshot.original_language.as_deref(), Some("ja"));
    let metrics = drain();
    assert_eq!(
        error_count(
            &metrics,
            &[
                ("provider", "anilist"),
                ("operation", "extras"),
                ("class", "unavailable")
            ]
        ),
        1
    );
}

#[tokio::test]
async fn a_mal_id_resolves_to_its_anilist_title_or_to_not_found() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            assert!(
                body["query"]
                    .as_str()
                    .is_some_and(|q| q.contains("StacksMalLookup"))
            );
            match body["variables"]["idMal"].as_i64() {
                Some(21) => ResponseTemplate::new(200)
                    .set_body_json(json!({"data": {"Media": {"id": 21, "format": "TV"}}})),
                Some(199) => ResponseTemplate::new(200)
                    .set_body_json(json!({"data": {"Media": {"id": 199, "format": "MOVIE"}}})),
                _ => ResponseTemplate::new(404).set_body_json(json!({
                    "data": {"Media": null},
                    "errors": [{"message": "Not Found.", "status": 404}]
                })),
            }
        })
        .mount(&anilist)
        .await;
    let p = adapter(&settings(&tmdb.uri(), &anilist.uri()));
    assert_eq!(
        p.anilist_by_mal_id(21).await.unwrap(),
        MediaId::new(Provider::AniList, MediaKind::Show, 21)
    );
    assert_eq!(
        p.anilist_by_mal_id(199).await.unwrap(),
        MediaId::new(Provider::AniList, MediaKind::Movie, 199)
    );
    let missing = p.anilist_by_mal_id(5).await.unwrap_err();
    assert_eq!(missing.kind, MetadataErrorKind::NotFound);
    assert_eq!(missing.operation, Operation::Extras);
    assert!(
        tmdb.received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}

#[tokio::test]
async fn tmdb_in_production_comes_from_the_status() {
    let _serial = SERIAL.lock().await;
    let tmdb = MockServer::start().await;
    let anilist = MockServer::start().await;
    // `in_production` is missing (cameo would default it to false) but the
    // show is still returning.
    Mock::given(path("/3/tv/1396"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1396, "name": "Show", "original_name": "Show", "overview": "x",
            "first_air_date": "2008-01-20", "genres": [], "origin_country": ["US"],
            "original_language": "en", "status": "Returning Series",
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
    let p = adapter(&settings(&tmdb.uri(), &anilist.uri()));
    let title = p
        .details(&MediaId::new(Provider::Tmdb, MediaKind::Show, 1396))
        .await
        .expect("ok");
    assert_eq!(title.detail.in_production, Some(true));
    assert_eq!(title.seasons.len(), 2);
    assert_eq!(title.seasons[1].episode_count, Some(10));
}

// ---------------------------------------------------------------- source rules

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable source dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn workspace_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("../api/src"), &mut files);
    files
}

/// Only the adapter (`provider.rs`) and its conversions (`mapping.rs`) may
/// name cameo; everything else goes through `MetadataProvider`.
#[test]
fn only_the_adapter_names_cameo() {
    let allowed = ["media/provider.rs", "media/mapping.rs"];
    let mut offenders = Vec::new();
    for file in workspace_sources() {
        let shown = file.to_string_lossy().replace('\\', "/");
        if allowed.iter().any(|a| shown.ends_with(a)) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("readable source");
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            if code.contains("cameo::") || code.trim_start().starts_with("use cameo") {
                offenders.push(format!("{shown}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "cameo named outside the adapter:\n{}",
        offenders.join("\n")
    );
}

/// No metadata result is swallowed: a `MetadataProvider` call is never
/// followed by `.ok()`, `.unwrap*()`, `.expect()`, `.is_ok()` or `.is_err()`.
#[test]
fn no_metadata_result_is_swallowed() {
    const CALLS: [&str; 5] = [
        "metadata.search(",
        "metadata.details(",
        "metadata.season(",
        "metadata.recommendations(",
        "metadata.feed(",
    ];
    const SWALLOW: [&str; 5] = [".ok()", ".unwrap", ".expect(", ".is_ok()", ".is_err()"];
    let mut offenders = Vec::new();
    for file in workspace_sources() {
        let text = std::fs::read_to_string(&file).expect("readable source");
        // Unit-test modules may unwrap freely.
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for call in CALLS {
            let mut rest = code;
            while let Some(at) = rest.find(call) {
                let after = &rest[at..];
                // The result is what follows the call's `.await`.
                if let Some(aw) = after.find(".await") {
                    let tail: String = after[aw + ".await".len()..]
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .take(12)
                        .collect();
                    if SWALLOW.iter().any(|s| tail.starts_with(s)) {
                        offenders.push(format!("{}: {call}… .await{tail}", file.display()));
                    }
                }
                rest = &after[call.len()..];
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "metadata results swallowed:\n{}",
        offenders.join("\n")
    );
}
