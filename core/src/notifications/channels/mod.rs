pub mod address;
pub mod apprise;
pub mod webhook;

pub use address::{AddressPolicy, METADATA_ADDRESSES, RefusedAddress};
pub use apprise::{AppriseChannel, AppriseChannelConfig};
pub use webhook::{WebhookChannel, WebhookChannelConfig};

use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::entity::enums::ChannelType;
use crate::notifications::{DeliveryFailure, NotificationError};

/// How long one delivery may take to connect, and to be answered in full.
/// Deliveries run inside the sequential job runner, so a receiver that never
/// answers must fail its delivery rather than hold every later job back.
pub const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

/// The process's address policy, once something has read or installed it.
static POLICY: OnceLock<Arc<AddressPolicy>> = OnceLock::new();

/// Make `policy` the one every delivery, and every channel saved, is held
/// to. Call it once, at startup, before anything is delivered; until then
/// the policy is the strictest one, which permits public addresses only.
///
/// # Errors
///
/// Hands `policy` back when a policy is already in force: installed before,
/// or read (and so fixed) by a delivery or a check.
pub fn install_address_policy(policy: AddressPolicy) -> Result<(), AddressPolicy> {
    POLICY.set(Arc::new(policy)).map_err(Arc::unwrap_or_clone)
}

/// The policy in force: the installed one, or else the strictest one.
pub fn address_policy() -> Arc<AddressPolicy> {
    Arc::clone(POLICY.get_or_init(|| Arc::new(uninstalled_policy())))
}

/// The policy when none was installed: the strictest one. Test suites
/// (which build with `test-support`, as no shipped build does) run their
/// receivers on `127.0.0.1`, so theirs also permits `127.0.0.0/8`; the
/// server always installs the configured policy, so this never applies to
/// it.
fn uninstalled_policy() -> AddressPolicy {
    let policy = AddressPolicy::default();
    #[cfg(feature = "test-support")]
    let policy = policy.exempting("127.0.0.0/8");
    policy
}

/// The HTTP client deliveries go through, the timeout it was built with (so
/// a timed-out delivery can say how long it waited), and the address policy
/// it connects under.
///
/// The client follows no redirect, since a receiver could otherwise send
/// the request on to an address the policy refuses, and uses no proxy, which
/// would connect to the receiver on Stacks' behalf where its address cannot
/// be checked.
#[derive(Debug, Clone)]
pub struct DeliveryClient {
    http: reqwest::Client,
    timeout: Duration,
    policy: Arc<AddressPolicy>,
}

impl DeliveryClient {
    /// The process-wide client, built once with [`DELIVERY_TIMEOUT`] under
    /// the [`address_policy`] in force. A `reqwest::Client` is a handle onto
    /// one connection pool, so the clone shares it.
    pub fn shared() -> Self {
        static SHARED: LazyLock<DeliveryClient> =
            LazyLock::new(|| DeliveryClient::with_timeout(DELIVERY_TIMEOUT));
        SHARED.clone()
    }

    /// A client whose connect and request timeouts are both `timeout`, under
    /// the [`address_policy`] in force.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self::with_policy(timeout, address_policy())
    }

    /// A client whose connect and request timeouts are both `timeout`, that
    /// connects only where `policy` permits.
    ///
    /// # Panics
    ///
    /// When the TLS backend cannot be initialised, as `reqwest::Client::new`
    /// does.
    pub fn with_policy(timeout: Duration, policy: Arc<AddressPolicy>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .dns_resolver(Arc::new(address::GuardedResolver {
                policy: Arc::clone(&policy),
            }))
            .build()
            .expect("the notification HTTP client can be built");
        Self {
            http,
            timeout,
            policy,
        }
    }

    /// The client to send a request to `url` with, once its host is known
    /// not to be an address literal the policy refuses. A literal never
    /// reaches the resolver that checks every other host.
    pub(crate) fn http_for(&self, url: &str) -> Result<&reqwest::Client, NotificationError> {
        match reqwest::Url::parse(url) {
            Ok(parsed) if !self.policy.permits_url(&parsed) => Err(refused(url)),
            _ => Ok(&self.http),
        }
    }

    /// The delivery error for a transport failure to `url`: a refused
    /// address, a timeout (saying how long the receiver was given), or
    /// anything else, which carries reqwest's own message and its causes. A
    /// webhook or Apprise URL carries its secret in the path or query, so
    /// the message names only the receiver's origin, never the URL reqwest
    /// would.
    pub(crate) fn send_error(&self, e: reqwest::Error, url: &str) -> NotificationError {
        if e.is_timeout() {
            return NotificationError::DeliveryFailed {
                failure: DeliveryFailure::Timeout,
                detail: format!(
                    "the receiver did not answer within {} seconds",
                    self.timeout.as_secs_f64()
                ),
            };
        }
        let e = e.without_url();
        let mut detail = format!("{e} to {}", origin(url));
        let mut source = std::error::Error::source(&e);
        while let Some(cause) = source {
            if cause.is::<RefusedAddress>() {
                return refused(url);
            }
            detail.push_str(": ");
            detail.push_str(&cause.to_string());
            source = cause.source();
        }
        NotificationError::DeliveryFailed {
            failure: DeliveryFailure::Unreachable,
            detail,
        }
    }
}

/// The delivery error for a receiver at an address the policy refuses.
fn refused(url: &str) -> NotificationError {
    NotificationError::DeliveryFailed {
        failure: DeliveryFailure::RefusedAddress,
        detail: format!("{RefusedAddress}: {}", origin(url)),
    }
}

/// The delivery error for a receiver that answered `status`.
pub(crate) fn http_error(channel: &str, status: reqwest::StatusCode) -> NotificationError {
    NotificationError::DeliveryFailed {
        failure: DeliveryFailure::HttpError,
        detail: format!("{channel} responded {status}"),
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

/// Whether a channel configuration's `url` may be saved under the
/// [`address_policy`] in force: a host that is an address literal must be one
/// it permits. A host name is judged each time it is delivered to, since what
/// it resolves to can change.
///
/// A restored backup is not held to this: its channel is kept, so nothing the
/// user configured is lost, and every delivery to it is refused instead.
pub(crate) fn check_address(config: &serde_json::Value) -> Result<(), NotificationError> {
    let url = config.get("url").and_then(serde_json::Value::as_str);
    match url.map(reqwest::Url::parse) {
        Some(Ok(url)) if !address_policy().permits_url(&url) => Err(
            NotificationError::InvalidConfig(format!("url: {RefusedAddress}")),
        ),
        _ => Ok(()),
    }
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

    /// A client that may deliver to the loopback receivers these tests run,
    /// which no shipped policy permits.
    fn loopback(timeout: Duration) -> DeliveryClient {
        DeliveryClient::with_policy(
            timeout,
            Arc::new(AddressPolicy::default().exempting("127.0.0.0/8")),
        )
    }

    /// The delivery failed in `failure`'s category, and `detail` holds.
    fn assert_failed(
        result: Result<(), NotificationError>,
        failure: DeliveryFailure,
        detail: impl Fn(&str) -> bool,
    ) {
        match result {
            Err(NotificationError::DeliveryFailed {
                failure: f,
                detail: d,
            }) => {
                assert_eq!(f, failure, "{d}");
                assert!(detail(&d), "{d}");
            }
            other => panic!("expected a {failure:?} failure, got {other:?}"),
        }
    }

    /// The delivery failed with the timeout's message.
    fn assert_timed_out(result: Result<(), NotificationError>) {
        assert_failed(result, DeliveryFailure::Timeout, |d| {
            d == "the receiver did not answer within 0.2 seconds"
        });
    }

    #[tokio::test]
    async fn a_webhook_receiver_that_never_answers_fails_the_delivery() {
        let url = silent_receiver().await;
        let channel = WebhookChannel::with_client(loopback(Duration::from_millis(200)));
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
        let channel = AppriseChannel::with_client(loopback(Duration::from_millis(200)));
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
            WebhookChannel::with_client(loopback(DELIVERY_TIMEOUT))
                .send(&message(), &json!({ "url": url }))
                .await,
            AppriseChannel::with_client(loopback(DELIVERY_TIMEOUT))
                .send(&message(), &json!({ "url": url }))
                .await,
        ] {
            assert_failed(result, DeliveryFailure::Unreachable, |d| {
                d.contains(&format!("to http://{addr}"))
                    && ["s3cr3t", "t0k", "pw"].iter().all(|s| !d.contains(s))
            });
        }
    }

    #[tokio::test]
    async fn an_internal_receiver_is_refused_before_anything_is_sent() {
        // A live receiver, so a refusal cannot be a failed connection.
        let receiver = wiremock::MockServer::start().await;
        let port = receiver.address().port();
        let strict = || DeliveryClient::with_policy(DELIVERY_TIMEOUT, Arc::default());
        for url in [
            format!("http://127.0.0.1:{port}/hook"),
            format!("http://[::ffff:127.0.0.1]:{port}/hook"),
            // A name is refused for what it resolves to, when it is sent.
            format!("http://localhost:{port}/hook"),
            "http://169.254.169.254/latest/meta-data/".to_owned(),
            "http://10.0.0.1/hook".to_owned(),
        ] {
            for result in [
                WebhookChannel::with_client(strict())
                    .send(&message(), &json!({ "url": url }))
                    .await,
                AppriseChannel::with_client(strict())
                    .send(&message(), &json!({ "url": url }))
                    .await,
            ] {
                assert_failed(result, DeliveryFailure::RefusedAddress, |d| {
                    !d.contains("/hook") && !d.contains("meta-data")
                });
            }
        }
        assert!(receiver.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_redirect_is_not_followed_and_fails_the_delivery() {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let receiver = MockServer::start().await;
        Mock::given(path("/hook"))
            .respond_with(ResponseTemplate::new(307).insert_header("location", "/elsewhere"))
            .mount(&receiver)
            .await;
        Mock::given(path("/elsewhere"))
            .respond_with(ResponseTemplate::new(204))
            .expect(0)
            .mount(&receiver)
            .await;
        let url = format!("{}/hook", receiver.uri());
        for result in [
            WebhookChannel::with_client(loopback(DELIVERY_TIMEOUT))
                .send(&message(), &json!({ "url": url }))
                .await,
            AppriseChannel::with_client(loopback(DELIVERY_TIMEOUT))
                .send(&message(), &json!({ "url": url }))
                .await,
        ] {
            assert_failed(result, DeliveryFailure::HttpError, |d| d.contains("307"));
        }
    }

    #[test]
    fn a_saved_address_literal_is_held_to_the_policy() {
        // No test in this crate installs a policy, so the uninstalled one
        // applies (which permits `127.0.0.0/8` under `test-support`).
        for url in [
            "http://169.254.169.254/",
            "http://[::1]/",
            "http://[::ffff:a9fe:a9fe]/",
            "http://192.168.1.2/",
        ] {
            assert!(matches!(
                check_address(&json!({ "url": url })),
                Err(NotificationError::InvalidConfig(_))
            ));
        }
        for config in [
            json!({ "url": "https://hooks.example/x" }),
            json!({ "url": "http://localhost/" }),
            json!({ "url": "not a url" }),
            json!({}),
        ] {
            assert!(check_address(&config).is_ok(), "{config}");
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

    #[test]
    fn a_policy_cannot_be_installed_once_one_is_in_force() {
        let in_force = address_policy();
        assert_eq!(
            install_address_policy(AddressPolicy::default()),
            Err(AddressPolicy::default())
        );
        assert!(Arc::ptr_eq(&in_force, &address_policy()));
    }
}
