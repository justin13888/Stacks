//! The request log carries the identifier the response does.
//!
//! Its own test binary, and the only test in it, because `tracing` caches
//! each callsite's interest process-wide. While a single dispatcher is
//! registered, a callsite's interest is decided by the default of whichever
//! thread first reaches it: a test on another thread serving a request with
//! no subscriber marks the response log's callsite as never enabled, and a
//! subscriber set here afterwards never sees that event. Alone in its
//! process, this test is the first and only thread to reach the request
//! log's callsites, and it reaches them under the capturing subscriber.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use kynos::http::StatusCode;
use serde_json::Value;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// Everything a subscriber writes, kept for the assertions.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
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

#[tokio::test]
async fn the_response_log_carries_the_request_id_the_response_does() {
    let env = env!();
    let client = env.client();

    let logs = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(logs.clone())
        .finish();
    // The test runtime is single-threaded, so the whole request is served
    // on this thread, under this subscriber.
    let guard = tracing::subscriber::set_default(subscriber);
    let response = client.get("/healthz").send().await;
    drop(guard);
    response.assert_status(StatusCode::OK);
    let assigned = response
        .header("x-request-id")
        .expect("RequestId assigns an X-Request-Id")
        .to_owned();
    assert!(!assigned.is_empty());

    let written = logs.0.lock().unwrap().clone();
    let events: Vec<Value> = String::from_utf8(written)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["fields"].clone())
        .collect();
    let event = |message: &str| {
        events
            .iter()
            .find(|fields| fields["message"] == message)
            .unwrap_or_else(|| panic!("no {message:?} event in {events:?}"))
            .clone()
    };
    let received = event("request received");
    assert_eq!(received["operation_id"], "getLiveness");
    assert!(received.get("request_id").is_none(), "{received}");
    let sent = event("response sent");
    assert_eq!(sent["request_id"], assigned.as_str(), "{sent}");
    client.assert_conformance();
}
