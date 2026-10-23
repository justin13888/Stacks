//! Notification channels: what a configuration must be, how a patch merges
//! into it, id-addressed subscriptions, and delivery with extra headers.

mod common;

use serde_json::json;
use stacks_core::entity::enums::{ChannelType, NotificationEventType};
use stacks_core::notifications::{
    ChannelUpdate, ConfigMerge, NewChannel, NotificationError, NotificationEvent,
    NotificationService,
};
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn webhook(url: &str) -> NewChannel {
    NewChannel {
        channel_type: ChannelType::Webhook,
        name: "hook".into(),
        enabled: true,
        config: json!({
            "url": url,
            "method": "PUT",
            "headers": { "Authorization": "Bearer from-config" },
            "secret": "s3cret"
        }),
    }
}

#[tokio::test]
async fn configurations_are_validated_and_merged() {
    let t = test_db!();
    let svc = NotificationService::new(t.db.clone());
    let user = common::user(&t.db).await;

    for config in [
        json!({ "url": "ftp://x.test/" }),
        json!({ "url": "https://x.test/", "method": "GET" }),
        json!({ "url": "https://x.test/", "secret": "" }),
        json!({ "url": "https://x.test/", "headers": { "Host": "evil" } }),
        json!({ "url": "https://x.test/", "unknown": true }),
        json!({}),
    ] {
        let refused = svc
            .create_channel(
                user.id,
                NewChannel {
                    config: config.clone(),
                    ..webhook("")
                },
            )
            .await;
        assert!(
            matches!(refused, Err(NotificationError::InvalidConfig(_))),
            "{config} was accepted"
        );
    }
    let channel = svc
        .create_channel(user.id, webhook("https://x.test/hook"))
        .await
        .unwrap();

    // A merge replaces what it names, removes what it nulls, keeps the rest.
    let merged = svc
        .update_channel(
            user.id,
            channel.id,
            ChannelUpdate {
                config: Some(ConfigMerge {
                    channel_type: ChannelType::Webhook,
                    members: json!({ "secret": null, "url": "https://y.test/" })
                        .as_object()
                        .unwrap()
                        .clone(),
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        merged.config,
        json!({
            "url": "https://y.test/",
            "method": "PUT",
            "headers": { "Authorization": "Bearer from-config" }
        })
    );
    // A merge that leaves an invalid configuration, or changes the type, is
    // refused and stores nothing.
    for (channel_type, members) in [
        (ChannelType::Webhook, json!({ "url": null })),
        (ChannelType::Apprise, json!({ "url": "https://a.test/" })),
    ] {
        let refused = svc
            .update_channel(
                user.id,
                channel.id,
                ChannelUpdate {
                    config: Some(ConfigMerge {
                        channel_type,
                        members: members.as_object().unwrap().clone(),
                    }),
                    ..Default::default()
                },
            )
            .await;
        assert!(matches!(refused, Err(NotificationError::InvalidConfig(_))));
    }
    assert_eq!(
        svc.get_channel(user.id, channel.id).await.unwrap().config,
        merged.config
    );
}

#[tokio::test]
async fn concurrent_merges_keep_every_member() {
    let t = test_db!();
    let svc = NotificationService::new(t.db.clone());
    let user = common::user(&t.db).await;
    let merge = |members: serde_json::Value| ChannelUpdate {
        config: Some(ConfigMerge {
            channel_type: ChannelType::Webhook,
            members: members.as_object().unwrap().clone(),
        }),
        ..Default::default()
    };
    for _ in 0..10 {
        let channel = svc
            .create_channel(
                user.id,
                NewChannel {
                    config: json!({ "url": "https://x.test/" }),
                    ..webhook("")
                },
            )
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            svc.update_channel(user.id, channel.id, merge(json!({ "secret": "s" }))),
            svc.update_channel(user.id, channel.id, merge(json!({ "method": "PUT" }))),
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(
            svc.get_channel(user.id, channel.id).await.unwrap().config,
            json!({ "url": "https://x.test/", "secret": "s", "method": "PUT" })
        );
    }
}

#[tokio::test]
async fn subscriptions_are_unique_per_channel_and_event_and_owned() {
    let t = test_db!();
    let svc = NotificationService::new(t.db.clone());
    let ada = common::user(&t.db).await;
    let eve = common::user(&t.db).await;
    let channel = svc
        .create_channel(ada.id, webhook("https://x.test/"))
        .await
        .unwrap();

    let sub = svc
        .create_subscription(ada.id, channel.id, NotificationEventType::NewEpisode, true)
        .await
        .unwrap();
    assert!(matches!(
        svc.create_subscription(ada.id, channel.id, NotificationEventType::NewEpisode, false)
            .await,
        Err(NotificationError::SubscriptionExists)
    ));
    assert!(matches!(
        svc.create_subscription(eve.id, channel.id, NotificationEventType::NewEpisode, true)
            .await,
        Err(NotificationError::ChannelNotFound)
    ));
    assert!(matches!(
        svc.update_subscription(eve.id, sub.id, Some(false)).await,
        Err(NotificationError::SubscriptionNotFound)
    ));
    assert!(
        !svc.update_subscription(ada.id, sub.id, Some(false))
            .await
            .unwrap()
            .enabled
    );
    assert!(matches!(
        svc.delete_subscription(eve.id, sub.id).await,
        Err(NotificationError::SubscriptionNotFound)
    ));
    svc.delete_subscription(ada.id, sub.id).await.unwrap();
    assert!(svc.list_subscriptions(ada.id).await.unwrap().is_empty());

    // Deleting a channel removes its subscriptions with it.
    svc.create_subscription(
        ada.id,
        channel.id,
        NotificationEventType::ImportCompleted,
        true,
    )
    .await
    .unwrap();
    svc.delete_channel(ada.id, channel.id).await.unwrap();
    assert!(svc.list_subscriptions(ada.id).await.unwrap().is_empty());
    assert_eq!(common::count(&t.db, "notification_subscriptions").await, 0);
}

#[tokio::test]
async fn a_webhook_is_delivered_with_its_method_headers_and_signature() {
    let t = test_db!();
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/hook"))
        .and(header("authorization", "Bearer from-config"))
        .and(header("content-type", "application/json"))
        .and(header_exists("x-stacks-signature"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let svc = NotificationService::new(t.db.clone());
    let user = common::user(&t.db).await;
    let channel = svc
        .create_channel(user.id, webhook(&format!("{}/hook", server.uri())))
        .await
        .unwrap();
    svc.create_subscription(
        user.id,
        channel.id,
        NotificationEventType::ImportCompleted,
        true,
    )
    .await
    .unwrap();

    svc.dispatch(
        user.id,
        NotificationEvent::ImportCompleted {
            import_id: "i".into(),
            total: 2,
            succeeded: 1,
            failed: 1,
        },
    )
    .await
    .unwrap();
    // Not subscribed to this one: nothing is sent.
    svc.dispatch(
        user.id,
        NotificationEvent::NewEpisode {
            cameo_media_id: "tmdb:show:1".into(),
            episode_id: "1".into(),
        },
    )
    .await
    .unwrap();
    server.verify().await;
}
