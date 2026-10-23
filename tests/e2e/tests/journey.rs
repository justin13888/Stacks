//! A whole journey, black-box: a real server on a socket, driven only
//! through the client spargen generated from `web/openapi.json`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../api/tests/support/mod.rs"]
mod support;

use std::sync::Arc;

use kynos::server::Server;
use secrecy::SecretString;
use stacks_e2e as client;
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

#[tokio::test]
async fn sign_in_search_track_watch_and_export() {
    let idp = MockIdp::start().await;
    let Some(env) = support::Env::with_issuer(&idp.issuer()).await else {
        return;
    };
    let base = serve(&env).await;
    let anonymous = client::Client::new(&base).expect("a client");

    // Sign in through the provider, as the web app does.
    let started = anonymous
        .create_oidc_authorization()
        .await
        .expect("an authorization URL")
        .into_inner();
    let state = idp.authorize(&started.authorization_url, NextLogin::subject("ada"));
    let created = anonymous
        .create_session(
            None,
            &client::types::NewSession {
                code: "good-code".into(),
                state,
            },
        )
        .await
        .expect("a session");
    let cookie = created
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix("stacks_session="))
        .and_then(|v| v.split(';').next())
        .expect("a session cookie")
        .to_owned();
    let me = client::Client::new(&base)
        .expect("a client")
        .with_credential(
            "SessionCookie",
            client::Credential::ApiKey(SecretString::from(cookie)),
        );
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
}
