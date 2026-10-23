//! Signing in through a mock OpenID Connect provider, and sessions.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use api::app::Settings;
use api::page::Page;
use api::sessions::{CurrentSession, OidcAuthorization, Session};
use kynos::http::StatusCode;
use kynos::test::TestClient;
use serde_json::json;
use stacks_core::entity::enums::Role;
use support::idp::{MockIdp, NextLogin};
use support::{Credentials, Env};

/// Start a sign-in and let the provider issue `next`. Returns the `state`.
async fn authorize(client: &TestClient<api::app::App>, idp: &MockIdp, next: NextLogin) -> String {
    let started: OidcAuthorization = client
        .post("/api/v1/auth/oidc/authorizations")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(started.authorization_url.starts_with(&idp.issuer()));
    idp.authorize(&started.authorization_url, next)
}

/// The session token a `createSession` response set.
fn session_cookie(response: &kynos::test::TestResponse) -> String {
    response
        .cookies()
        .into_iter()
        .find(|(name, _)| *name == api::security::SESSION_COOKIE)
        .map(|(_, value)| value.to_owned())
        .expect("a session cookie")
}

#[tokio::test]
async fn signing_in_verifies_the_id_token_and_sets_an_http_only_cookie() {
    let idp = MockIdp::start().await;
    let Some(env) = Env::with_issuer(&idp.issuer()).await else {
        return;
    };
    let client = env.client();

    // A first sign-in provisions the account and starts a session.
    let state = authorize(&client, &idp, NextLogin::subject("ada")).await;
    let created = client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .header("user-agent", "Firefox")
        .send()
        .await;
    created.assert_status(StatusCode::CREATED);
    assert_eq!(created.header("location"), Some("/api/v1/sessions/current"));
    let set_cookie = created.header("set-cookie").expect("a Set-Cookie field");
    for attribute in ["HttpOnly", "SameSite=Lax", "Path=/api", "Max-Age=3600"] {
        assert!(
            set_cookie.contains(attribute),
            "{attribute} missing from {set_cookie}"
        );
    }
    assert!(
        !set_cookie.contains("Secure"),
        "plain HTTP must not mark the cookie Secure"
    );
    let body: CurrentSession = created.json();
    assert_eq!(body.user.display_name, "ada");
    assert_eq!(body.session.user_agent.as_deref(), Some("Firefox"));
    assert!(body.session.current);
    let token = session_cookie(&created);

    // The cookie is the credential, and nothing else is.
    let current: CurrentSession = client
        .get("/api/v1/sessions/current")
        .cookie(api::security::SESSION_COOKIE, &token)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(current.user.id, body.user.id);

    // A state is good once.
    client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("sign-in-failed"));

    // A second sign-in finds the same account.
    let state = authorize(&client, &idp, NextLogin::subject("ada")).await;
    let again: CurrentSession = client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    assert_eq!(again.user.id, body.user.id);

    // An address on the admin allowlist signs in as an administrator.
    let mut root = NextLogin::subject("root");
    root.email = Some("root@example.test".into());
    let state = authorize(&client, &idp, root).await;
    let admin: CurrentSession = client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    assert_eq!(admin.user.role, api::account::Role::Admin);

    client.assert_conformance();
}

#[tokio::test]
async fn a_forged_or_misdirected_sign_in_is_refused() {
    let idp = MockIdp::start().await;
    let Some(env) = Env::with_issuer(&idp.issuer()).await else {
        return;
    };
    let client = env.client();
    let refused = |state: String| {
        let client = &client;
        async move {
            client
                .post("/api/v1/sessions")
                .json(&json!({ "code": "good-code", "state": state }))
                .send()
                .await
                .assert_status(StatusCode::BAD_REQUEST)
                .assert_problem_type(&support::problem("sign-in-failed"));
        }
    };

    // Signed with a key the provider does not publish.
    let mut forged = NextLogin::subject("eve");
    forged.foreign_key = true;
    refused(authorize(&client, &idp, forged).await).await;

    // Issued for another client.
    let mut misdirected = NextLogin::subject("eve");
    misdirected.wrong_audience = true;
    refused(authorize(&client, &idp, misdirected).await).await;

    // Bound to another authorization request.
    let mut replayed = NextLogin::subject("eve");
    replayed.nonce_override = Some("some-other-nonce".into());
    refused(authorize(&client, &idp, replayed).await).await;

    // A code the provider refuses (the token endpoint checks PKCE too).
    let state = authorize(&client, &idp, NextLogin::subject("eve")).await;
    client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "stolen-code", "state": state }))
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // A state nobody issued.
    refused("made-up".into()).await;

    // A provider that fails mid-sign-in is a 502, not a failed sign-in.
    let mut down = NextLogin::subject("eve");
    down.token_endpoint_down = true;
    let state = authorize(&client, &idp, down).await;
    client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await
        .assert_status(StatusCode::BAD_GATEWAY)
        .assert_problem_type(&support::problem("identity-provider-unavailable"));

    // A deactivated account cannot sign in.
    let user = env.user(Role::User).await;
    env.deactivate(&user).await;
    let mut deactivated = NextLogin::subject(&user.user.oidc_subject);
    deactivated.email = None;
    let state = authorize(&client, &idp, deactivated).await;
    client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await
        .assert_status(StatusCode::FORBIDDEN)
        .assert_problem_type(&support::problem("account-deactivated"));

    client.assert_conformance();
}

#[tokio::test]
async fn a_secure_deployment_marks_the_cookie_secure() {
    let idp = MockIdp::start().await;
    let Some(env) = Env::with_issuer(&idp.issuer()).await else {
        return;
    };
    let client = env.client_with(Settings {
        secure_cookies: true,
        ..support::settings()
    });
    let state = authorize(&client, &idp, NextLogin::subject("ada")).await;
    let created = client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await;
    created.assert_status(StatusCode::CREATED);
    assert!(
        created
            .header("set-cookie")
            .expect("a cookie")
            .contains("Secure")
    );
    client.assert_conformance();
}

#[tokio::test]
async fn an_unreachable_provider_is_a_502() {
    // The default environment's issuer accepts no connections.
    let env = env!();
    let client = env.client();
    client
        .post("/api/v1/auth/oidc/authorizations")
        .send()
        .await
        .assert_status(StatusCode::BAD_GATEWAY)
        .assert_problem_type(&support::problem("identity-provider-unavailable"));
    client.assert_conformance();
}

#[tokio::test]
async fn sessions_are_listed_and_revoked() {
    let idp = MockIdp::start().await;
    let Some(env) = Env::with_issuer(&idp.issuer()).await else {
        return;
    };
    let client = env.client();
    let ada = env.user(Role::User).await;
    let laptop = env.user(Role::User).await;

    // A second session of the same account, from a sign-in.
    let mut next = NextLogin::subject(&ada.user.oidc_subject);
    next.email = None;
    let state = authorize(&client, &idp, next).await;
    let created = client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await;
    created.assert_status(StatusCode::CREATED);
    let phone_token = session_cookie(&created);
    let phone: CurrentSession = created.json();

    let page: Page<Session> = client
        .get("/api/v1/sessions")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items.iter().filter(|s| s.current).count(), 1);
    // Through a token no session is current.
    let page: Page<Session> = client
        .get("/api/v1/sessions?limit=1")
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(page.items.len(), 1);
    assert!(page.items.iter().all(|s| !s.current));
    let next_page: Page<Session> = client
        .get(&format!(
            "/api/v1/sessions?limit=1&cursor={}",
            page.next_cursor.expect("a second page")
        ))
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(next_page.items.len(), 1);
    assert_eq!(next_page.next_cursor, None);
    client
        .get("/api/v1/sessions?cursor=not-a-cursor")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));

    // Ending a session takes effect on the next request.
    client
        .delete(&format!("/api/v1/sessions/{}", phone.session.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client
        .get("/api/v1/sessions/current")
        .cookie(api::security::SESSION_COOKIE, &phone_token)
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    // Someone else's session is not yours to end.
    let theirs: CurrentSession = client
        .get("/api/v1/sessions/current")
        .session(&laptop)
        .send()
        .await
        .json();
    client
        .delete(&format!("/api/v1/sessions/{}", theirs.session.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("session-not-found"));

    // Signing out clears the cookie and ends the session.
    let out = client
        .delete("/api/v1/sessions/current")
        .session(&ada)
        .send()
        .await;
    out.assert_status(StatusCode::NO_CONTENT);
    assert!(
        out.header("set-cookie")
            .is_some_and(|c| c.starts_with("stacks_session=;") && c.contains("Max-Age=0"))
    );
    client
        .get("/api/v1/me")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    // Tokens are not sessions: they outlive signing out, and cannot end one.
    client
        .get("/api/v1/me")
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .delete("/api/v1/sessions/current")
        .bearer(&ada.read_write)
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // The provider's two failure modes, so this environment observes every
    // response the area declares.
    let unreachable = env.client_with_issuer("http://127.0.0.1:9");
    unreachable
        .post("/api/v1/auth/oidc/authorizations")
        .send()
        .await
        .assert_status(StatusCode::BAD_GATEWAY);
    unreachable.assert_conformance();
    let mut down = NextLogin::subject("eve");
    down.token_endpoint_down = true;
    let state = authorize(&client, &idp, down).await;
    client
        .post("/api/v1/sessions")
        .json(&json!({ "code": "good-code", "state": state }))
        .send()
        .await
        .assert_status(StatusCode::BAD_GATEWAY);

    client.assert_conformance();
    support::sweep::sweep(&env, "sessions").await;
    env.assert_covered("sessions");
}
