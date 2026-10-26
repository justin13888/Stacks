//! No credential reaches a log line when a metadata call fails.
//!
//! A TMDB v3 key travels as `?api_key=<key>`, and reqwest names the request
//! URL in its error text, which cameo passes on. Each test points the
//! adapter at a port nothing listens on, captures everything it logs as the
//! JSON the API writes, and asserts the key is nowhere in it.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use stacks_core::media::breaker::BreakerConfig;
use stacks_core::media::provider::{RateSettings, RetrySettings};
use stacks_core::media::{
    CameoProvider, DiscoverCategory, MediaId, MediaKind, MetadataErrorKind, MetadataProvider,
    Provider, ProviderSettings, UnavailableCause,
};
use tokio::net::TcpListener;

/// A v3-shaped key, so cameo sends it in the query.
const KEY: &str = "0123456789abcdef0123456789abcdef";

/// Everything the subscriber writes.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().expect("log lock").clone()).expect("UTF-8 logs")
    }
}

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// `http://127.0.0.1:PORT` where nothing listens: every connection is refused.
async fn closed_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("address");
    drop(listener);
    format!("http://{addr}")
}

fn settings(base: &str) -> ProviderSettings {
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
        tmdb_base_url: Some(base.to_owned()),
        anilist_base_url: Some(format!("{base}/graphql")),
        tmdb_language: Some("en-US".to_owned()),
    }
}

fn is_transport(kind: &MetadataErrorKind) -> bool {
    matches!(
        kind,
        MetadataErrorKind::Unavailable(UnavailableCause::Transport(_))
    )
}

#[tokio::test]
async fn a_tmdb_transport_failure_logs_no_api_key() {
    let logs = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let p = CameoProvider::new(KEY, &settings(&closed_port().await)).expect("adapter builds");
    let movie = MediaId::new(Provider::Tmdb, MediaKind::Movie, 550);
    let show = MediaId::new(Provider::Tmdb, MediaKind::Show, 1396);
    let errors = [
        p.search(Provider::Tmdb, "fight", 1).await.map(drop),
        p.details(&movie).await.map(drop),
        p.details(&show).await.map(drop),
        p.season(&show, 1).await.map(drop),
        p.recommendations(&movie, 1).await.map(drop),
        p.feed(
            Provider::Tmdb,
            DiscoverCategory::Trending,
            MediaKind::Movie,
            1,
        )
        .await
        .map(drop),
    ];
    for result in errors {
        let err = result.expect_err("nothing is listening");
        assert!(is_transport(&err.kind), "{err}");
        assert!(!err.to_string().contains(KEY), "{err}");
    }

    let text = logs.text();
    assert!(text.contains("metadata provider unavailable"), "{text}");
    // The URL is still there to diagnose with, its key replaced.
    assert!(text.contains("api_key=<redacted>"), "{text}");
    assert!(!text.contains(KEY), "{text}");
}

#[tokio::test]
async fn stacks_own_anilist_queries_log_no_url() {
    let logs = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let p = CameoProvider::new(KEY, &settings(&closed_port().await)).expect("adapter builds");
    let err = p
        .anilist_by_mal_id(1)
        .await
        .expect_err("nothing is listening");
    assert!(is_transport(&err.kind), "{err}");

    let text = logs.text();
    assert!(text.contains("metadata provider unavailable"), "{text}");
    assert!(!text.contains("for url"), "{text}");
}
