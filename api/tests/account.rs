//! The caller's account: profile, preferences and API tokens.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use api::account::{ApiToken, CreatedApiToken, Theme, User};
use api::page::Page;
use kynos::http::StatusCode;
use serde_json::json;
use stacks_core::entity::enums::Role;
use support::Credentials;

#[tokio::test]
async fn the_profile_reads_and_merges() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;

    let me: User = client
        .get("/api/v1/me")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(me.id, ada.id());
    assert_eq!(me.preferences.theme, Theme::System);

    // A token reads with `read`, and only with `read`.
    client
        .get("/api/v1/me")
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .get("/api/v1/me")
        .bearer(&ada.write)
        .send()
        .await
        .assert_status(StatusCode::FORBIDDEN)
        .assert_problem_type(&support::problem("insufficient-scope"));

    // Merge semantics: members left out are untouched.
    let renamed: User = client
        .patch("/api/v1/me")
        .session(&ada)
        .json(&json!({ "display_name": "Ada", "preferences": { "theme": "dark" } }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(renamed.display_name, "Ada");
    assert_eq!(renamed.preferences.theme, Theme::Dark);
    let themed: User = client
        .patch("/api/v1/me")
        .bearer(&ada.write)
        .json(&json!({ "preferences": { "title_language": "romaji" } }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(themed.display_name, "Ada");
    assert_eq!(themed.preferences.theme, Theme::Dark);
    assert_eq!(
        themed.preferences.title_language,
        api::account::TitleLanguage::Romaji
    );
    let unchanged: User = client
        .patch("/api/v1/me")
        .session(&ada)
        .json(&json!({}))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(unchanged.display_name, "Ada");

    // A blank name is refused; so is a value no preference has.
    client
        .patch("/api/v1/me")
        .session(&ada)
        .json(&json!({ "display_name": "   " }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .assert_problem_type(&support::problem("invalid-display-name"));
    client
        .patch("/api/v1/me")
        .session(&ada)
        .json(&json!({ "preferences": { "theme": "neon" } }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    // Malformed JSON is the client's mistake, never a 500.
    client
        .patch("/api/v1/me")
        .session(&ada)
        .body("application/json", "{\"display_name\":")
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    client
        .patch("/api/v1/me")
        .session(&ada)
        .body("text/plain", "display_name=Ada")
        .send()
        .await
        .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);

    client.assert_conformance();
}

#[tokio::test]
async fn tokens_are_created_shown_once_scoped_and_revoked() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let root = env.user(Role::Admin).await;

    let created = client
        .post("/api/v1/me/api-tokens")
        .session(&ada)
        .json(&json!({ "name": "backup script", "scopes": ["read"] }))
        .send()
        .await;
    created.assert_status(StatusCode::CREATED);
    let created_body: CreatedApiToken = created.json();
    assert!(created_body.secret.starts_with("stk_"));
    assert_eq!(
        created.header("location"),
        Some(format!("/api/v1/me/api-tokens/{}", created_body.token.id).as_str())
    );

    // The secret authenticates, and is never shown again.
    client
        .get("/api/v1/me")
        .bearer(&created_body.secret)
        .send()
        .await
        .assert_status(StatusCode::OK);
    let one: ApiToken = client
        .get(&format!("/api/v1/me/api-tokens/{}", created_body.token.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(one.name, "backup script");
    let listed: Page<ApiToken> = client
        .get("/api/v1/me/api-tokens")
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    // The fixture's three tokens and this one.
    assert_eq!(listed.items.len(), 4);
    assert!(
        !client
            .get("/api/v1/me/api-tokens")
            .session(&ada)
            .send()
            .await
            .text()
            .contains(&created_body.secret)
    );

    // A read token cannot write.
    client
        .patch("/api/v1/me")
        .bearer(&created_body.secret)
        .json(&json!({ "display_name": "Eve" }))
        .send()
        .await
        .assert_status(StatusCode::FORBIDDEN)
        .assert_problem_type(&support::problem("insufficient-scope"));
    // A token cannot mint another token: only a browser session can.
    client
        .post("/api/v1/me/api-tokens")
        .bearer(&ada.read_write)
        .json(&json!({ "name": "escalate", "scopes": ["write"] }))
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    // Only an administrator grants `admin`.
    client
        .post("/api/v1/me/api-tokens")
        .session(&ada)
        .json(&json!({ "name": "root", "scopes": ["admin"] }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .assert_problem_type(&support::problem("scope-not-permitted"));
    client
        .post("/api/v1/me/api-tokens")
        .session(&root)
        .json(&json!({ "name": "root", "scopes": ["admin"] }))
        .send()
        .await
        .assert_status(StatusCode::CREATED);
    for invalid in [
        json!({ "name": "   ", "scopes": ["read"] }),
        json!({ "name": "late", "scopes": ["read"], "expires_at": "2001-01-01T00:00:00Z" }),
    ] {
        client
            .post("/api/v1/me/api-tokens")
            .session(&ada)
            .json(&invalid)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-api-token"));
    }

    // Revoking takes effect on the next request, and is not repeatable.
    client
        .delete(&format!("/api/v1/me/api-tokens/{}", created_body.token.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client
        .get("/api/v1/me")
        .bearer(&created_body.secret)
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    client
        .delete(&format!("/api/v1/me/api-tokens/{}", created_body.token.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("api-token-not-found"));
    client
        .get(&format!("/api/v1/me/api-tokens/{}", created_body.token.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    // Another account's token is not yours to see or revoke.
    let theirs: Page<ApiToken> = client
        .get("/api/v1/me/api-tokens")
        .session(&root)
        .send()
        .await
        .json();
    let theirs = theirs.items.first().expect("root has tokens").id;
    client
        .get(&format!("/api/v1/me/api-tokens/{theirs}"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);

    client.assert_conformance();

    // A change the database cannot store is a 500, and says nothing more.
    let degraded = env.degraded_client(support::db::Credential::Session).await;
    let failed = degraded
        .patch("/api/v1/me")
        .session(&ada)
        .json(&json!({ "display_name": "Ada Lovelace" }))
        .send()
        .await;
    failed
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR)
        .assert_problem_type(&support::problem("internal"));
    assert!(!failed.text().contains("permission denied"));
    degraded.assert_conformance();

    support::sweep::sweep(&env, "account").await;
    env.assert_covered("account");
}
