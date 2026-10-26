//! Adding a title never overwrites an entry already in the library: a
//! create-only store (`If-None-Match: *`) answers 412, and restoring a live
//! entry answers 409, both leaving the entry exactly as it was.

use super::support::metadata::{BREAKING_BAD, FIGHT_CLUB};
use super::support::{self, Credentials, Env, TestUser};
use super::{Client, entry_url, get, put, refused};
use api::catalog::Status;
use api::library::{EntryEvent, LibraryEntry, LibraryEntryDetail};
use api::page::Page;
use kynos::http::StatusCode;
use kynos::test::TestResponse;
use serde_json::{Value, json};
use stacks_core::entity::enums::Role;

/// A create-only store of `reference`, answering `expect`.
async fn add_only(
    client: &Client,
    user: &TestUser,
    reference: &str,
    body: Value,
    expect: StatusCode,
) -> TestResponse {
    let response = client
        .put(&entry_url(reference))
        .session(user)
        .header("If-None-Match", "*")
        .json(&body)
        .send()
        .await;
    response.assert_status(expect);
    response
}

pub(super) async fn create_only(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let full = json!({ "status": "completed", "score": 90, "favourite": true, "notes": "keep" });

    // A title not in the library is created, as without the precondition.
    let created: LibraryEntry =
        add_only(&client, &ada, FIGHT_CLUB, full.clone(), StatusCode::CREATED)
            .await
            .json();
    assert_eq!(
        (created.status, created.score, created.favourite),
        (Status::Completed, Some(90), true)
    );
    let before: LibraryEntryDetail = get(&client, &ada, &entry_url(FIGHT_CLUB)).await;
    let events_url = format!("{}/events", entry_url(FIGHT_CLUB));
    let events_before: Page<EntryEvent> = get(&client, &ada, &events_url).await;

    // A second add (a stale card, a double click) is refused and changes
    // nothing: not the members, not the runs, not the event log.
    add_only(
        &client,
        &ada,
        FIGHT_CLUB,
        json!({ "status": "planning" }),
        StatusCode::PRECONDITION_FAILED,
    )
    .await
    .assert_problem_type(&support::problem("entry-exists"));
    // Restoring a live entry is a conflict, not a missing trash entry.
    refused(
        &client,
        &ada,
        "post",
        &format!("{}/restorations", entry_url(FIGHT_CLUB)),
        None,
        StatusCode::CONFLICT,
        "entry-live",
    )
    .await;
    let after: LibraryEntryDetail = get(&client, &ada, &entry_url(FIGHT_CLUB)).await;
    assert_eq!(after, before);
    let events_after: Page<EntryEvent> = get(&client, &ada, &events_url).await;
    assert_eq!(events_after, events_before);

    // Only `*` is evaluated: the API issues no entity tags, so a tag list
    // never matches and the store replaces the entry.
    let replaced: LibraryEntry = client
        .put(&entry_url(FIGHT_CLUB))
        .session(&ada)
        .header("If-None-Match", "\"some-tag\"")
        .json(&json!({ "status": "paused" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        (replaced.status, replaced.score, replaced.favourite),
        (Status::Paused, None, false)
    );

    // A title in the trash counts as absent: the add brings it back.
    put(
        &client,
        &ada,
        BREAKING_BAD,
        json!({ "status": "watching" }),
        StatusCode::CREATED,
    )
    .await;
    client
        .delete(&entry_url(BREAKING_BAD))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let back: LibraryEntry = add_only(
        &client,
        &ada,
        BREAKING_BAD,
        json!({ "status": "planning" }),
        StatusCode::CREATED,
    )
    .await
    .json();
    assert_eq!((back.status, back.deleted_at), (Status::Planning, None));

    client.assert_conformance();
}

#[tokio::test]
async fn adding_a_title_never_overwrites_its_entry() {
    let env = crate::env!();
    create_only(&env).await;
}
