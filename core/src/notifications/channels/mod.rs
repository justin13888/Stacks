pub mod apprise;
pub mod webhook;

pub use apprise::{AppriseChannel, AppriseChannelConfig};
pub use webhook::{WebhookChannel, WebhookChannelConfig};

use std::sync::LazyLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::entity::enums::ChannelType;
use crate::notifications::NotificationError;

/// How long one delivery may take to connect, and to be answered in full.
/// Deliveries run inside the sequential job runner, so a receiver that never
/// answers must fail its delivery rather than hold every later job back.
pub const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// The HTTP client deliveries go through, and the timeout it was built with
/// (so a timed-out delivery can say how long it waited).
#[derive(Debug, Clone)]
pub struct DeliveryClient {
    http: reqwest::Client,
    timeout: Duration,
}

impl DeliveryClient {
    /// The process-wide client, built once with [`DELIVERY_TIMEOUT`]. A
    /// `reqwest::Client` is a handle onto one connection pool, so the clone
    /// shares it.
    pub fn shared() -> Self {
        static SHARED: LazyLock<DeliveryClient> =
            LazyLock::new(|| DeliveryClient::with_timeout(DELIVERY_TIMEOUT));
        SHARED.clone()
    }

    /// A client whose connect and request timeouts are both `timeout`.
    ///
    /// # Panics
    ///
    /// When the TLS backend cannot be initialised, as `reqwest::Client::new`
    /// does.
    pub fn with_timeout(timeout: Duration) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
            .expect("the notification HTTP client can be built");
        Self { http, timeout }
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// The delivery error for a transport failure to `url`: a timeout says
    /// how long the receiver was given, anything else carries reqwest's own
    /// message and its causes. A webhook or Apprise URL carries its secret
    /// in the path or query, so the message names only the receiver's
    /// origin, never the URL reqwest would.
    pub(crate) fn send_error(&self, e: reqwest::Error, url: &str) -> NotificationError {
        if e.is_timeout() {
            return NotificationError::DeliveryFailed(format!(
                "the receiver did not answer within {} seconds",
                self.timeout.as_secs_f64()
            ));
        }
        let e = e.without_url();
        let mut message = format!("{e} to {}", origin(url));
        let mut source = std::error::Error::source(&e);
        while let Some(cause) = source {
            message.push_str(": ");
            message.push_str(&cause.to_string());
            source = cause.source();
        }
        NotificationError::DeliveryFailed(message)
    }
}

/// `scheme://host[:port]` of a delivery URL: what a log or an error may name
/// of it. Its user information, path and query may hold a secret.
pub(crate) fn origin(raw: &str) -> String {
    reqwest::Url::parse(raw)
        .ok()
        .and_then(|url| {
            let host = url.host_str()?.to_owned();
            Some(match url.port() {
                Some(port) => format!("{}://{host}:{port}", url.scheme()),
                None => format!("{}://{host}", url.scheme()),
            })
        })
        .unwrap_or_else(|| "an unparseable URL".to_owned())
}

/// A notification rendered to a human-readable form, ready to deliver. The
/// [`NotificationService`](super::NotificationService) resolves the source
/// event into this shape (reading titles from the catalog snapshot) so
/// individual channels only have to think about transport, not content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedNotification {
    pub event_type: String,
    pub title: String,
    pub body: String,
}

/// Deliver one notification through a channel of `channel_type` configured
/// by `config`, on the shared [`DeliveryClient`], so the delivery is bounded
/// by [`DELIVERY_TIMEOUT`]. Every delivery, test or real, goes through here.
pub(crate) async fn deliver(
    channel_type: ChannelType,
    message: &RenderedNotification,
    config: &serde_json::Value,
) -> Result<(), NotificationError> {
    match channel_type {
        ChannelType::Webhook => WebhookChannel::new().send(message, config).await,
        ChannelType::Apprise => AppriseChannel::new().send(message, config).await,
    }
}

/// A delivery URL must be absolute `http` or `https` with a host.
pub(crate) fn validate_url(raw: &str) -> Result<(), NotificationError> {
    let url = reqwest::Url::parse(raw)
        .map_err(|e| NotificationError::InvalidConfig(format!("url: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(NotificationError::InvalidConfig(
            "url must be an absolute http or https URL".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::net::TcpListener;

    use super::*;

    /// A receiver that accepts every connection and never answers: the
    /// accepted sockets are held open for as long as the test runs.
    async fn silent_receiver() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });
        format!("http://{addr}/hook")
    }

    fn message() -> RenderedNotification {
        RenderedNotification {
            event_type: "test".into(),
            title: "t".into(),
            body: "b".into(),
        }
    }

    /// The delivery failed with the timeout's message.
    fn assert_timed_out(result: Result<(), NotificationError>) {
        match result {
            Err(NotificationError::DeliveryFailed(msg)) => {
                assert_eq!(msg, "the receiver did not answer within 0.2 seconds");
            }
            other => panic!("expected a timed-out delivery, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_webhook_receiver_that_never_answers_fails_the_delivery() {
        let url = silent_receiver().await;
        let channel =
            WebhookChannel::with_client(DeliveryClient::with_timeout(Duration::from_millis(200)));
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            channel.send(&message(), &json!({ "url": url })),
        )
        .await
        .expect("the client's own timeout ends the delivery");
        assert_timed_out(result);
    }

    #[tokio::test]
    async fn an_apprise_receiver_that_never_answers_fails_the_delivery() {
        let url = silent_receiver().await;
        let channel =
            AppriseChannel::with_client(DeliveryClient::with_timeout(Duration::from_millis(200)));
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            channel.send(&message(), &json!({ "url": url })),
        )
        .await
        .expect("the client's own timeout ends the delivery");
        assert_timed_out(result);
    }

    #[tokio::test]
    async fn a_failed_delivery_names_the_origin_and_never_the_secret() {
        // A port nothing listens on: the connection is refused.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://user:pw@{addr}/notify/s3cr3t?token=t0k");
        for result in [
            WebhookChannel::new()
                .send(&message(), &json!({ "url": url }))
                .await,
            AppriseChannel::new()
                .send(&message(), &json!({ "url": url }))
                .await,
        ] {
            match result {
                Err(NotificationError::DeliveryFailed(msg)) => {
                    assert!(msg.contains(&format!("to http://{addr}")), "{msg}");
                    for secret in ["s3cr3t", "t0k", "pw"] {
                        assert!(!msg.contains(secret), "{msg}");
                    }
                }
                other => panic!("expected a failed delivery, got {other:?}"),
            }
        }
    }

    #[test]
    fn an_origin_is_scheme_host_and_port_only() {
        assert_eq!(
            origin("https://u:p@hooks.example:8443/a/b?c=d"),
            "https://hooks.example:8443"
        );
        assert_eq!(origin("http://apprise/notify/key"), "http://apprise");
        assert_eq!(origin("not a url"), "an unparseable URL");
    }

    #[test]
    fn the_shared_client_is_bounded_by_the_delivery_timeout() {
        assert_eq!(DeliveryClient::shared().timeout, DELIVERY_TIMEOUT);
        assert_eq!(DELIVERY_TIMEOUT, Duration::from_secs(10));
    }
}
