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

    /// The delivery error for a transport failure: a timeout says how long
    /// the receiver was given, anything else carries reqwest's own message.
    pub(crate) fn send_error(&self, e: reqwest::Error) -> NotificationError {
        if e.is_timeout() {
            NotificationError::DeliveryFailed(format!(
                "the receiver did not answer within {} seconds",
                self.timeout.as_secs_f64()
            ))
        } else {
            NotificationError::DeliveryFailed(e.to_string())
        }
    }
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

    #[test]
    fn the_shared_client_is_bounded_by_the_delivery_timeout() {
        assert_eq!(DeliveryClient::shared().timeout, DELIVERY_TIMEOUT);
        assert_eq!(DELIVERY_TIMEOUT, Duration::from_secs(10));
    }
}
