//! A whole journey, black-box: a real server on a socket, driven only
//! through the client spargen generated from `web/openapi.json`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use kynos::server::Server;
use secrecy::SecretString;
use stacks_e2e as client;
use stacks_test_support as support;
use support::idp::{MockIdp, NextLogin};

/// Serve the real router on an ephemeral port; returns its base URL.
async fn serve(env: &support::Env) -> String {
    let settings = support::settings();
    let db = env.db.db.clone();
    let app = api::app::App {
        auth: Arc::new(env.auth(&db)),
        db,
        metadata: Arc::new(env.metadata.clone()),
        settings: Arc::new(settings.clone()),
    };
    let service = api::app::router(&settings)
        .build(app)
        .expect("the router builds");
    let server = Server::new(service)
        .bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .prepare()
        .await
        .expect("bind an ephemeral port");
    let addr = *server.local_addrs().first().expect("a bound address");
    tokio::spawn(server.serve());
    format!("http://{addr}")
}

/// The value of the cookie `name` a response set.
fn set_cookie(headers: &reqwest::header::HeaderMap, name: &str) -> String {
    let prefix = format!("{name}=");
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix(prefix.as_str()))
        .and_then(|v| v.split(';').next())
        .unwrap_or_else(|| panic!("a {name} cookie"))
        .to_owned()
}

/// Sign `subject` in through the provider, as the web app does; returns a
/// client carrying the session cookie.
async fn sign_in(idp: &MockIdp, base: &str, subject: &str) -> client::Client {
    let anonymous = client::Client::new(base).expect("a client");
    let started = anonymous
        .create_oidc_authorization()
        .await
        .expect("an authorization URL");
    // The browser keeps the sign-in cookie and sends it back to finish.
    let login = set_cookie(started.headers(), "stacks_login");
    let started = started.into_inner();
    let state = idp.authorize(&started.authorization_url, NextLogin::subject(subject));
    let created = anonymous
        .create_session(
            client::CreateSessionParams {
                stacks_login: Some(login),
                ..Default::default()
            },
            &client::types::NewSession {
                code: "good-code".into(),
                state,
            },
        )
        .await
        .expect("a session");
    let cookie = set_cookie(created.headers(), "stacks_session");
    client::Client::new(base)
        .expect("a client")
        .with_credential(
            "SessionCookie",
            client::Credential::ApiKey(SecretString::from(cookie)),
        )
}

#[tokio::test]
async fn sign_in_search_track_watch_and_export() {
    let idp = MockIdp::start().await;
    let Some(env) = support::Env::with_issuer(&idp.issuer()).await else {
        return;
    };
    let base = serve(&env).await;
    let me = sign_in(&idp, &base, "ada").await;
    let current = me
        .get_current_session()
        .await
        .expect("the session")
        .into_inner();
    assert_eq!(current.user.display_name, "ada");

    // Search.
    let found = me
        .search_titles("breaking", None)
        .await
        .expect("search results")
        .into_inner();
    assert!(found.items.iter().any(|t| t.title == "Breaking Bad"));

    // Track a movie: a new entry answers 201.
    let put = me
        .put_library_entry(
            support::metadata::FIGHT_CLUB,
            None,
            &client::types::EntryBody {
                status: client::types::Status::Planning,
                score: None,
                favourite: None,
                notes: None,
            },
        )
        .await
        .expect("the entry")
        .into_inner();
    assert!(matches!(put, client::PutLibraryEntryResponse::Status201(_)));

    // Watch it, then take the play back.
    let logged = me
        .log_plays(
            support::metadata::FIGHT_CLUB,
            &client::types::NewPlays::NewPlaysVariant0(Box::new(client::types::NewPlaysVariant0 {
                kind: client::types::NewPlaysVariant0kind::Movie,
                watched_at: None,
            })),
        )
        .await
        .expect("a logged play")
        .into_inner();
    let [play] = logged.plays.as_slice() else {
        panic!("one play, got {}", logged.plays.len());
    };
    me.void_play(play.id).await.expect("the play voided");

    let library = me
        .list_library(None)
        .await
        .expect("the library")
        .into_inner();
    assert!(
        library
            .items
            .iter()
            .any(|e| e.title.r#ref == support::metadata::FIGHT_CLUB)
    );

    // Export. The backup is a file in the Stacks backup format, which the
    // API describes as JSON and docs/backup-format.schema.json details.
    let backup = me
        .export_backup(client::ExportBackupParams::default())
        .await
        .expect("a backup")
        .into_inner();
    let backup: stacks_core::import_export::backup::Backup =
        serde_json::from_value(serde_json::to_value(backup).expect("JSON"))
            .expect("a Stacks backup document");
    let fight_club = backup
        .titles
        .iter()
        .find(|t| t.name == "Fight Club")
        .expect("the tracked title is in the backup");
    assert!(backup.entries.iter().any(|e| e.title_id == fight_club.id));
}

/// The request encodes and the server creates the channel, but spargen 0.5
/// matches an inline `oneOf` branch's tag against a name it invented, so the
/// `ChannelConfig` in the response cannot be decoded.
#[tokio::test]
#[ignore = "spargen 0.5 cannot decode an inline discriminated union: getkono/spargen#1001"]
async fn a_notification_channel_round_trips() {
    let idp = MockIdp::start().await;
    let Some(env) = support::Env::with_issuer(&idp.issuer()).await else {
        return;
    };
    let base = serve(&env).await;
    let me = sign_in(&idp, &base, "ada").await;

    let channel = me
        .create_notification_channel(&client::types::NewChannelBody {
            name: "hook".into(),
            enabled: None,
            config: client::types::NewChannelConfig::NewChannelConfigVariant0(Box::new(
                client::types::NewChannelConfigVariant0 {
                    r#type: client::types::NewChannelConfigVariant0type::Webhook,
                    url: "https://hooks.example.com/stacks".into(),
                    method: None,
                    headers: None,
                    secret: None,
                },
            )),
        })
        .await
        .expect("a channel")
        .into_inner();
    assert_eq!(channel.name, "hook");
}
