//! Notification channels and the subscriptions routing events to them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use api::notifications::{Channel, ChannelConfig, NotificationEvent, Subscription, WebhookMethod};
use api::page::Page;
use kynos::http::StatusCode;
use kynos::test::TestClient;
use serde_json::{Value, json};
use stacks_core::entity::enums::Role;
use support::{Credentials, TestUser};
use uuid::Uuid;

fn webhook() -> Value {
    json!({
        "name": "  Home server  ",
        "config": {
            "type": "webhook",
            "url": "https://hooks.example.test/stacks",
            "method": "PUT",
            "headers": { "Authorization": "Bearer hush-header" },
            "secret": "hush-secret"
        }
    })
}

async fn create(client: &TestClient<api::app::App>, user: &TestUser, body: &Value) -> Channel {
    client
        .post("/api/v1/notification-channels")
        .session(user)
        .json(body)
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json()
}

#[tokio::test]
async fn channels_are_created_read_merged_and_deleted_without_leaking_secrets() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let eve = env.user(Role::User).await;

    let created = client
        .post("/api/v1/notification-channels")
        .bearer(&ada.write)
        .json(&webhook())
        .send()
        .await;
    created.assert_status(StatusCode::CREATED);
    assert!(!created.text().contains("hush"));
    let channel: Channel = created.json();
    assert_eq!(
        created.header("location"),
        Some(format!("/api/v1/notification-channels/{}", channel.id).as_str())
    );
    assert_eq!(channel.name, "Home server");
    assert!(channel.enabled);
    assert_eq!(
        channel.config,
        ChannelConfig::Webhook {
            url: "https://hooks.example.test/stacks".into(),
            method: WebhookMethod::Put,
            header_names: vec!["Authorization".into()],
            has_secret: true,
        }
    );
    let apprise = create(
        &client,
        &ada,
        &json!({
            "name": "Phone",
            "enabled": false,
            "config": { "type": "apprise", "url": "http://apprise.local/notify/k", "tags": ["phone"] }
        }),
    )
    .await;
    assert!(!apprise.enabled);

    let read = client
        .get(&format!("/api/v1/notification-channels/{}", channel.id))
        .bearer(&ada.read)
        .send()
        .await;
    read.assert_status(StatusCode::OK);
    assert!(!read.text().contains("hush"));
    assert_eq!(read.json::<Channel>(), channel);
    let listed: Page<Channel> = client
        .get("/api/v1/notification-channels")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        listed.items.iter().map(|c| c.id).collect::<Vec<_>>(),
        [apprise.id, channel.id]
    );
    let first: Page<Channel> = client
        .get("/api/v1/notification-channels?limit=1")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(first.items.len(), 1);
    assert!(first.next_cursor.is_some());

    // Another account sees none of it.
    let theirs: Page<Channel> = client
        .get("/api/v1/notification-channels")
        .session(&eve)
        .send()
        .await
        .json();
    assert!(theirs.items.is_empty());
    client
        .get(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&eve)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("notification-channel-not-found"));

    // Merge: the name alone leaves the configuration; a config member alone
    // leaves the others; `null` clears.
    let renamed: Channel = client
        .patch(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&ada)
        .json(&json!({ "name": "NAS" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(renamed.name, "NAS");
    assert_eq!(renamed.config, channel.config);
    let unsigned: Channel = client
        .patch(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&ada)
        .json(&json!({
            "enabled": false,
            "config": { "type": "webhook", "secret": null, "method": null }
        }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(!unsigned.enabled);
    assert_eq!(
        unsigned.config,
        ChannelConfig::Webhook {
            url: "https://hooks.example.test/stacks".into(),
            method: WebhookMethod::Post,
            header_names: vec!["Authorization".into()],
            has_secret: false,
        }
    );
    let retagged: Channel = client
        .patch(&format!("/api/v1/notification-channels/{}", apprise.id))
        .session(&ada)
        .json(&json!({ "config": { "type": "apprise", "tags": null } }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        retagged.config,
        ChannelConfig::Apprise {
            url: "http://apprise.local/notify/k".into(),
            tags: vec![],
        }
    );

    // What core cannot deliver to is refused, on create and on merge.
    for invalid in [
        json!({ "name": "x", "config": { "type": "webhook", "url": "ftp://files.example.test/" } }),
        json!({ "name": "x", "config": { "type": "webhook", "url": "not a url" } }),
        json!({ "name": "x", "config": { "type": "webhook", "url": "https://h.example.test/",
                "headers": { "Content-Type": "text/plain" } } }),
        json!({ "name": "x", "config": { "type": "webhook", "url": "https://h.example.test/",
                "headers": { "bad header": "x" } } }),
        json!({ "name": "   ", "config": { "type": "apprise", "url": "https://a.example.test/" } }),
        json!({ "name": "x", "config": { "type": "apprise", "url": "https://a.example.test/",
                "tags": [" "] } }),
        // An address Stacks never delivers to, however it is spelled.
        json!({ "name": "x", "config": { "type": "webhook",
                "url": "http://169.254.169.254/latest/meta-data/" } }),
        json!({ "name": "x", "config": { "type": "apprise", "url": "http://[::ffff:10.0.0.1]/" } }),
        json!({ "name": "x", "config": { "type": "webhook", "url": "http://[fe80::1]:9095/" } }),
    ] {
        client
            .post("/api/v1/notification-channels")
            .session(&ada)
            .json(&invalid)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-notification-channel"));
    }
    for invalid in [
        json!({ "config": { "type": "apprise", "url": "https://a.example.test/" } }),
        json!({ "config": { "type": "webhook", "url": "mailto:ada@example.test" } }),
        json!({ "config": { "type": "webhook", "url": "http://192.168.1.10/hook" } }),
        json!({ "name": "  " }),
    ] {
        client
            .patch(&format!("/api/v1/notification-channels/{}", channel.id))
            .session(&ada)
            .json(&invalid)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-notification-channel"));
    }
    // A member no channel type has is the schema's refusal.
    client
        .post("/api/v1/notification-channels")
        .session(&ada)
        .json(&json!({ "name": "x", "config": { "type": "email", "url": "https://x.test/" } }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    client
        .patch(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&eve)
        .json(&json!({ "name": "mine now" }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("notification-channel-not-found"));

    client
        .delete(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&eve)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    client
        .delete(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client
        .get(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);

    client.assert_conformance();
}

/// A test delivery reports how the receiver answered. Run inside the test
/// that asserts the area's coverage, which counts its responses.
async fn test_deliveries(env: &support::Env) {
    use wiremock::matchers::{body_partial_json, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let client = env.client();
    let ada = env.user(Role::User).await;
    let eve = env.user(Role::User).await;
    let receiver = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/ok"))
        .and(header_exists("x-stacks-signature"))
        .and(body_partial_json(json!({ "event_type": "test" })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&receiver)
        .await;
    Mock::given(method("POST"))
        .and(path("/refuses"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&receiver)
        .await;
    let channel = |name: &str, url: String, method: &str, enabled: bool| {
        json!({
            "name": name,
            "enabled": enabled,
            "config": { "type": "webhook", "url": url, "method": method, "secret": "s" }
        })
    };

    // A disabled channel is tried all the same: the test is explicit.
    let works = create(
        &client,
        &ada,
        &channel("Works", format!("{}/ok", receiver.uri()), "PUT", false),
    )
    .await;
    client
        .post(&format!("/api/v1/notification-channels/{}/tests", works.id))
        .bearer(&ada.write)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let refuses = create(
        &client,
        &ada,
        &channel(
            "Refuses",
            format!("{}/refuses", receiver.uri()),
            "POST",
            true,
        ),
    )
    .await;
    let refused = client
        .post(&format!(
            "/api/v1/notification-channels/{}/tests",
            refuses.id
        ))
        .session(&ada)
        .send()
        .await;
    refused
        .assert_status(StatusCode::BAD_GATEWAY)
        .assert_problem_type(&support::problem("notification-delivery-failed"));
    // The category, and nothing the receiver said.
    assert_eq!(refused.json::<Value>()["category"], "http-error");
    assert!(!refused.text().contains("503"), "{}", refused.text());

    // Nothing listens on a port the OS just handed back.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let unreachable = create(
        &client,
        &ada,
        &channel("Gone", format!("http://{closed}/hook"), "POST", true),
    )
    .await;
    let gone = client
        .post(&format!(
            "/api/v1/notification-channels/{}/tests",
            unreachable.id
        ))
        .session(&ada)
        .send()
        .await;
    gone.assert_status(StatusCode::BAD_GATEWAY)
        .assert_problem_type(&support::problem("notification-delivery-failed"));
    assert_eq!(gone.json::<Value>()["category"], "unreachable");
    assert!(
        !gone.text().contains(&closed.to_string()),
        "{}",
        gone.text()
    );

    // A channel saved before its address was refused (a restored backup's,
    // say) is kept, and every delivery to it is refused.
    {
        use sea_orm::{ActiveModelTrait, ActiveValue::Set};
        use stacks_core::entity::notification_channel;
        let internal = create(&client, &ada, &webhook()).await;
        notification_channel::ActiveModel {
            id: sea_orm::ActiveValue::Unchanged(internal.id),
            config: Set(json!({ "url": "http://169.254.169.254/latest/meta-data/" })),
            ..Default::default()
        }
        .update(&env.db.db)
        .await
        .unwrap();
        let refused = client
            .post(&format!(
                "/api/v1/notification-channels/{}/tests",
                internal.id
            ))
            .session(&ada)
            .send()
            .await;
        refused
            .assert_status(StatusCode::BAD_GATEWAY)
            .assert_problem_type(&support::problem("notification-delivery-failed"));
        assert_eq!(refused.json::<Value>()["category"], "refused-address");
        assert!(!refused.text().contains("169.254"), "{}", refused.text());
    }

    // Another account's channel is not the caller's to test.
    client
        .post(&format!("/api/v1/notification-channels/{}/tests", works.id))
        .session(&eve)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("notification-channel-not-found"));
    client
        .post(&format!(
            "/api/v1/notification-channels/{}/tests",
            Uuid::now_v7()
        ))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    client.assert_conformance();
}

#[tokio::test]
async fn subscriptions_route_each_event_to_a_channel_once() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let eve = env.user(Role::User).await;
    let channel = create(&client, &ada, &webhook()).await;
    let eves = create(&client, &eve, &webhook()).await;

    let created = client
        .post("/api/v1/notification-subscriptions")
        .bearer(&ada.write)
        .json(&json!({ "channel_id": channel.id, "event": "new_episode" }))
        .send()
        .await;
    created.assert_status(StatusCode::CREATED);
    let sub: Subscription = created.json();
    assert_eq!(
        created.header("location"),
        Some(format!("/api/v1/notification-subscriptions/{}", sub.id).as_str())
    );
    assert_eq!(sub.channel_id, channel.id);
    assert_eq!(sub.event, NotificationEvent::NewEpisode);
    assert!(sub.enabled);
    let other: Subscription = client
        .post("/api/v1/notification-subscriptions")
        .session(&ada)
        .json(&json!({ "channel_id": channel.id, "event": "import_completed", "enabled": false }))
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    assert!(!other.enabled);

    // One subscription per channel and event.
    client
        .post("/api/v1/notification-subscriptions")
        .session(&ada)
        .json(&json!({ "channel_id": channel.id, "event": "new_episode", "enabled": false }))
        .send()
        .await
        .assert_status(StatusCode::CONFLICT)
        .assert_problem_type(&support::problem("subscription-exists"));
    // Only to the caller's own channels.
    for channel_id in [eves.id, Uuid::now_v7()] {
        client
            .post("/api/v1/notification-subscriptions")
            .session(&ada)
            .json(&json!({ "channel_id": channel_id, "event": "new_episode" }))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("notification-channel-not-found"));
    }
    // An unknown event, and `upcoming_release`, which nothing ever raises.
    for event in ["everything", "upcoming_release"] {
        client
            .post("/api/v1/notification-subscriptions")
            .session(&ada)
            .json(&json!({ "channel_id": channel.id, "event": event }))
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let listed: Page<Subscription> = client
        .get("/api/v1/notification-subscriptions")
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        listed.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        [other.id, sub.id]
    );

    let paused: Subscription = client
        .patch(&format!("/api/v1/notification-subscriptions/{}", sub.id))
        .session(&ada)
        .json(&json!({ "enabled": false }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(!paused.enabled);
    let untouched: Subscription = client
        .patch(&format!("/api/v1/notification-subscriptions/{}", sub.id))
        .session(&ada)
        .json(&json!({}))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(!untouched.enabled);
    client
        .patch(&format!("/api/v1/notification-subscriptions/{}", sub.id))
        .session(&eve)
        .json(&json!({ "enabled": true }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("notification-subscription-not-found"));

    client
        .delete(&format!("/api/v1/notification-subscriptions/{}", sub.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client
        .delete(&format!("/api/v1/notification-subscriptions/{}", sub.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("notification-subscription-not-found"));

    // Deleting a channel takes its subscriptions with it.
    client
        .delete(&format!("/api/v1/notification-channels/{}", channel.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let left: Page<Subscription> = client
        .get("/api/v1/notification-subscriptions")
        .session(&ada)
        .send()
        .await
        .json();
    assert!(left.items.is_empty());

    client.assert_conformance();

    // The statuses the generic sweep cannot reach, in this environment.
    let fresh = create(&client, &ada, &webhook()).await;
    client
        .get(&format!("/api/v1/notification-channels/{}", fresh.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .patch(&format!("/api/v1/notification-channels/{}", fresh.id))
        .session(&ada)
        .json(&json!({ "config": { "type": "apprise" } }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    client
        .patch(&format!("/api/v1/notification-channels/{}", fresh.id))
        .session(&ada)
        .json(&json!({}))
        .send()
        .await
        .assert_status(StatusCode::OK);
    let again: Subscription = client
        .post("/api/v1/notification-subscriptions")
        .session(&ada)
        .json(&json!({ "channel_id": fresh.id, "event": "import_completed" }))
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    client
        .post("/api/v1/notification-subscriptions")
        .session(&ada)
        .json(&json!({ "channel_id": fresh.id, "event": "import_completed" }))
        .send()
        .await
        .assert_status(StatusCode::CONFLICT);
    client
        .patch(&format!("/api/v1/notification-subscriptions/{}", again.id))
        .session(&ada)
        .json(&json!({ "enabled": true }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .delete(&format!("/api/v1/notification-subscriptions/{}", again.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client
        .delete(&format!("/api/v1/notification-channels/{}", fresh.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client.assert_conformance();

    // The sweep's minimal channel is not deliverable, so it never reaches the
    // database; a valid one does, and fails there.
    let degraded = env.degraded_client(support::db::Credential::Session).await;
    let failed = degraded
        .post("/api/v1/notification-channels")
        .session(&ada)
        .json(&webhook())
        .send()
        .await;
    failed
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR)
        .assert_problem_type(&support::problem("internal"));
    degraded.assert_conformance();

    test_deliveries(&env).await;
    support::sweep::sweep(&env, "notifications").await;
    env.assert_covered("notifications");
}
