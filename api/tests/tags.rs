//! Tags, and putting them on titles in the library.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use api::app::App;
use api::page::Page;
use api::tags::Tag;
use kynos::http::StatusCode;
use kynos::test::TestClient;
use serde_json::{Value, json};
use stacks_core::entity::enums::Role;
use stacks_core::library::{EntryPatch, LibraryService};
use stacks_core::media::MediaService;
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, REPLY_1988};
use support::{Credentials, Env, TestUser};

/// Put a title in `user`'s library, as the library area would.
async fn track(env: &Env, user: &TestUser, raw: &str) -> uuid::Uuid {
    let row = MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()))
        .ensure_in_catalog(&metadata::id(raw))
        .await
        .expect("a snapshot");
    LibraryService::new(env.db.db.clone())
        .upsert(user.id(), row.id, EntryPatch::default())
        .await
        .expect("an entry");
    row.id
}

async fn create(client: &TestClient<App>, user: &TestUser, body: Value) -> Tag {
    let response = client
        .post("/api/v1/tags")
        .session(user)
        .json(&body)
        .send()
        .await;
    response.assert_status(StatusCode::CREATED);
    let tag: Tag = response.json();
    assert_eq!(
        response.header("location"),
        Some(format!("/api/v1/tags/{}", tag.id).as_str())
    );
    tag
}

async fn read(client: &TestClient<App>, user: &TestUser, tag: &Tag) -> Tag {
    client
        .get(&format!("/api/v1/tags/{}", tag.id))
        .bearer(&user.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
}

/// Tags are created, merged, paged and deleted, and stay private.
async fn tag_lifecycle(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let bob = env.user(Role::User).await;

    let red = create(
        &client,
        &ada,
        json!({ "name": " red ", "color": "#ff0000" }),
    )
    .await;
    assert_eq!(red.name, "red");
    assert_eq!(red.color.as_deref(), Some("#ff0000"));
    assert_eq!(red.entry_count, 0);
    assert_eq!(read(&client, &ada, &red).await, red);
    let plain: Tag = client
        .post("/api/v1/tags")
        .bearer(&ada.write)
        .json(&json!({ "name": "plain" }))
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    assert_eq!(plain.color, None);

    // Names are unique per user, not across users.
    client
        .post("/api/v1/tags")
        .session(&ada)
        .json(&json!({ "name": "red" }))
        .send()
        .await
        .assert_status(StatusCode::CONFLICT)
        .assert_problem_type(&support::problem("tag-name-taken"));
    create(&client, &bob, json!({ "name": "red" })).await;
    for body in [
        json!({ "name": "  " }),
        json!({ "name": "blue", "color": "blue" }),
        json!({ "name": "blue", "color": "#12345" }),
    ] {
        client
            .post("/api/v1/tags")
            .session(&ada)
            .json(&body)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-tag"));
    }

    // Merge: a rename keeps the colour, `null` clears it, `{}` changes
    // nothing; a taken name, a blank one and a bad colour are refused.
    let url = format!("/api/v1/tags/{}", red.id);
    let renamed: Tag = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "name": "crimson" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(renamed.name, "crimson");
    assert_eq!(renamed.color.as_deref(), Some("#ff0000"));
    let recoloured: Tag = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "color": "#00Aa00" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(recoloured.name, "crimson");
    assert_eq!(recoloured.color.as_deref(), Some("#00Aa00"));
    let cleared: Tag = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "color": null }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(cleared.color, None);
    assert_eq!(cleared.name, "crimson");
    let unchanged: Tag = client
        .patch(&url)
        .session(&ada)
        .json(&json!({}))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(unchanged, cleared);
    client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "name": "plain" }))
        .send()
        .await
        .assert_status(StatusCode::CONFLICT)
        .assert_problem_type(&support::problem("tag-name-taken"));
    for body in [json!({ "name": " " }), json!({ "color": "#zzzzzz" })] {
        client
            .patch(&url)
            .session(&ada)
            .json(&body)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-tag"));
    }

    // Newest first, paged.
    let first: Page<Tag> = client
        .get("/api/v1/tags?limit=1")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(first.items[0].id, plain.id);
    let next = first.next_cursor.clone().expect("a second page");
    let second: Page<Tag> = client
        .get(&format!("/api/v1/tags?limit=1&cursor={next}"))
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(second.items[0].id, red.id);
    assert_eq!(second.next_cursor, None);
    client
        .get("/api/v1/tags?cursor=eA")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));

    // Another user's tag does not exist for them.
    track(env, &bob, FIGHT_CLUB).await;
    for request in [
        client.get(&url),
        client.patch(&url).json(&json!({ "name": "mine" })),
        client.delete(&url),
        client.put(&format!("/api/v1/library/{FIGHT_CLUB}/tags/{}", red.id)),
        client.delete(&format!("/api/v1/library/{FIGHT_CLUB}/tags/{}", red.id)),
    ] {
        request
            .session(&bob)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("tag-not-found"));
    }

    // Deleted once.
    client
        .delete(&url)
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    for request in [client.get(&url), client.delete(&url)] {
        request
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("tag-not-found"));
    }

    client.assert_conformance();
}

/// Tagging needs a live library entry, is idempotent both ways, and
/// deleting a tag leaves the entries.
async fn tagging(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let tag = create(&client, &ada, json!({ "name": "rewatch" })).await;
    let fight_club = track(env, &ada, FIGHT_CLUB).await;
    track(env, &ada, BREAKING_BAD).await;

    let on = |reference: &str| format!("/api/v1/library/{reference}/tags/{}", tag.id);
    for _ in 0..2 {
        for reference in [FIGHT_CLUB, BREAKING_BAD] {
            client
                .put(&on(reference))
                .session(&ada)
                .send()
                .await
                .assert_status(StatusCode::NO_CONTENT);
        }
    }
    assert_eq!(read(&client, &ada, &tag).await.entry_count, 2);
    let listed: Page<Tag> = client
        .get("/api/v1/tags")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(listed.items[0].entry_count, 2);

    // A title never stored, and one stored but not in this library.
    MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()))
        .ensure_in_catalog(&metadata::id(REPLY_1988))
        .await
        .unwrap();
    for reference in ["tmdb:movie:31337", REPLY_1988] {
        for request in [client.put(&on(reference)), client.delete(&on(reference))] {
            request
                .session(&ada)
                .send()
                .await
                .assert_status(StatusCode::NOT_FOUND)
                .assert_problem_type(&support::problem("entry-not-found"));
        }
    }
    // An entry in the trash is not taggable, and its tags stop counting.
    LibraryService::new(env.db.db.clone())
        .delete(ada.id(), fight_club)
        .await
        .unwrap();
    client
        .put(&on(FIGHT_CLUB))
        .bearer(&ada.write)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("entry-not-found"));
    assert_eq!(read(&client, &ada, &tag).await.entry_count, 1);

    // Untagging twice is fine.
    for _ in 0..2 {
        client
            .delete(&on(BREAKING_BAD))
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    assert_eq!(read(&client, &ada, &tag).await.entry_count, 0);

    // Deleting a tag removes its assignments, not the entries.
    client
        .put(&on(BREAKING_BAD))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client
        .delete(&format!("/api/v1/tags/{}", tag.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let library = LibraryService::new(env.db.db.clone());
    let breaking_bad =
        stacks_core::media::catalog::find_by_ref(&env.db.db, &metadata::id(BREAKING_BAD))
            .await
            .unwrap()
            .unwrap();
    assert!(
        library
            .get(ada.id(), breaking_bad.id)
            .await
            .unwrap()
            .is_some()
    );
    let again = create(&client, &ada, json!({ "name": "rewatch" })).await;
    assert_eq!(again.entry_count, 0);

    client.assert_conformance();
}

#[tokio::test]
async fn tags_are_created_merged_and_deleted() {
    let env = env!();
    tag_lifecycle(&env).await;
}

#[tokio::test]
async fn tags_go_on_library_entries() {
    let env = env!();
    tagging(&env).await;
}

#[tokio::test]
async fn tags_cover_every_declared_response() {
    let env = env!();
    tag_lifecycle(&env).await;
    tagging(&env).await;
    support::sweep::sweep(&env, "tags").await;
    env.assert_covered("tags");
}
