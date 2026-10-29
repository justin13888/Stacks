//! Lists: manual lists ordered by hand, and smart lists a filter fills.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "lists/smart.rs"]
mod smart;
mod support;

use api::app::App;
use api::catalog::Kind;
use api::lists::{List, ListItem, ListItemPage};
use api::page::Page;
use kynos::http::StatusCode;
use kynos::test::TestClient;
use serde_json::{Value, json};
use stacks_core::entity::enums::Role;
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, ONE_PIECE, REPLY_1988};
use support::{Credentials, Env, TestUser};

async fn create(client: &TestClient<App>, user: &TestUser, body: Value) -> List {
    let response = client
        .post("/api/v1/lists")
        .session(user)
        .json(&body)
        .send()
        .await;
    response.assert_status(StatusCode::CREATED);
    let list: List = response.json();
    assert_eq!(
        response.header("location"),
        Some(format!("/api/v1/lists/{}", list.id).as_str())
    );
    list
}

async fn items(client: &TestClient<App>, user: &TestUser, list: &List) -> Vec<ListItem> {
    let page: ListItemPage = client
        .get(&format!("/api/v1/lists/{}/items", list.id))
        .session(user)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    page.items
}

fn refs(items: &[ListItem]) -> Vec<String> {
    items
        .iter()
        .map(|i| i.title.reference.to_string())
        .collect()
}

async fn add(
    client: &TestClient<App>,
    user: &TestUser,
    list: &List,
    body: Value,
) -> kynos::test::TestResponse {
    client
        .post(&format!("/api/v1/lists/{}/items", list.id))
        .session(user)
        .json(&body)
        .send()
        .await
}

async fn patch_item(
    client: &TestClient<App>,
    user: &TestUser,
    list: &List,
    reference: &str,
    body: Value,
) -> kynos::test::TestResponse {
    client
        .patch(&format!("/api/v1/lists/{}/items/{reference}", list.id))
        .session(user)
        .json(&body)
        .send()
        .await
}

/// Lists are created, read, changed by merge, trashed and restored.
async fn list_lifecycle(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let bob = env.user(Role::User).await;

    let list = create(
        &client,
        &ada,
        json!({ "name": "  Favourites ", "description": "the best" }),
    )
    .await;
    assert_eq!(list.name, "Favourites");
    assert!(!list.smart);
    assert_eq!(list.filter, None);
    assert_eq!(list.item_count, Some(0));
    let read: List = client
        .get(&format!("/api/v1/lists/{}", list.id))
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(read, list);

    // A token creates with `write`.
    let scripted: List = client
        .post("/api/v1/lists")
        .bearer(&ada.write)
        .json(&json!({ "name": "Scripted" }))
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    assert_eq!(scripted.description, None);

    // A blank name is refused.
    client
        .post("/api/v1/lists")
        .session(&ada)
        .json(&json!({ "name": "   " }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .assert_problem_type(&support::problem("invalid-list"));

    // Merge: a rename leaves the description, `null` clears it, `{}` is a
    // no-op, and a manual list has no filter to set.
    let url = format!("/api/v1/lists/{}", list.id);
    let renamed: List = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "name": "Faves" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(renamed.name, "Faves");
    assert_eq!(renamed.description.as_deref(), Some("the best"));
    let cleared: List = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "description": null }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(cleared.description, None);
    assert_eq!(cleared.name, "Faves");
    let unchanged: List = client
        .patch(&url)
        .session(&ada)
        .json(&json!({}))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(unchanged.name, "Faves");
    for body in [json!({ "filter": {} }), json!({ "name": " " })] {
        client
            .patch(&url)
            .session(&ada)
            .json(&body)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-list"));
    }

    // Newest first, paged.
    let all: Page<List> = client
        .get("/api/v1/lists")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        all.items.iter().map(|l| l.id).collect::<Vec<_>>(),
        [scripted.id, list.id]
    );
    let first: Page<List> = client
        .get("/api/v1/lists?limit=1")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    let next = first.next_cursor.clone().expect("a second page");
    let second: Page<List> = client
        .get(&format!("/api/v1/lists?limit=1&cursor={next}"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(second.items[0].id, list.id);
    assert_eq!(second.next_cursor, None);
    client
        .get("/api/v1/lists?cursor=bm9wZQ")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));

    // Another user's list does not exist for them.
    for request in [
        client.get(&url),
        client.patch(&url).json(&json!({ "name": "Mine" })),
        client.delete(&url),
        client.post(&format!("{url}/restorations")),
        client.get(&format!("{url}/items")),
        client
            .post(&format!("{url}/items"))
            .json(&json!({ "ref": FIGHT_CLUB })),
        client
            .patch(&format!("{url}/items/{FIGHT_CLUB}"))
            .json(&json!({ "notes": "x" })),
        client.delete(&format!("{url}/items/{FIGHT_CLUB}")),
    ] {
        request
            .session(&bob)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("list-not-found"));
    }

    // Soft delete: gone from the live lists, in the trash with its items,
    // and back on restore.
    add(&client, &ada, &list, json!({ "ref": FIGHT_CLUB }))
        .await
        .assert_status(StatusCode::CREATED);
    client
        .delete(&url)
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    client
        .get(&url)
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    client
        .delete(&url)
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    let live: Page<List> = client
        .get("/api/v1/lists")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        live.items.iter().map(|l| l.id).collect::<Vec<_>>(),
        [scripted.id]
    );
    let trash: Page<List> = client
        .get("/api/v1/lists?deleted=true")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(trash.items.len(), 1);
    assert!(trash.items[0].deleted_at.is_some());
    assert_eq!(trash.items[0].item_count, Some(1));
    let restored: List = client
        .post(&format!("{url}/restorations"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(restored.deleted_at, None);
    assert_eq!(restored.item_count, Some(1));
    // Restoring a live list changes nothing.
    client
        .post(&format!("{url}/restorations"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK);
    assert_eq!(refs(&items(&client, &ada, &restored).await), [FIGHT_CLUB]);
    client
        .post(&format!(
            "/api/v1/lists/{}/restorations",
            uuid::Uuid::now_v7()
        ))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("list-not-found"));

    client.assert_conformance();
}

/// A manual list's titles: added where asked, moved without drift, noted,
/// paged, removed.
async fn manual_items(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let list = create(&client, &ada, json!({ "name": "Queue" })).await;

    for reference in [FIGHT_CLUB, BREAKING_BAD, REPLY_1988] {
        let response = add(&client, &ada, &list, json!({ "ref": reference })).await;
        response.assert_status(StatusCode::CREATED);
        assert_eq!(
            response.header("location"),
            Some(format!("/api/v1/lists/{}/items", list.id).as_str())
        );
    }
    let added: ListItem = add(
        &client,
        &ada,
        &list,
        json!({ "ref": ONE_PIECE, "notes": "long", "before": BREAKING_BAD }),
    )
    .await
    .assert_status(StatusCode::CREATED)
    .json();
    assert_eq!(added.title.title, "One Piece");
    assert_eq!(added.notes.as_deref(), Some("long"));
    assert!(added.added_at.is_some());
    let now = items(&client, &ada, &list).await;
    assert_eq!(
        refs(&now),
        [FIGHT_CLUB, ONE_PIECE, BREAKING_BAD, REPLY_1988]
    );
    // The position is an opaque string the client can sort by.
    let mut positions: Vec<String> = now.iter().map(|i| i.position.clone().unwrap()).collect();
    let in_order = positions.clone();
    positions.sort();
    assert_eq!(positions, in_order);

    // Twice is a conflict; a title the provider lacks is missing; an anchor
    // outside the list is missing; neighbours that are not are refused.
    add(&client, &ada, &list, json!({ "ref": FIGHT_CLUB }))
        .await
        .assert_status(StatusCode::CONFLICT)
        .assert_problem_type(&support::problem("already-in-list"));
    env.metadata.set_missing("tmdb:movie:404");
    add(&client, &ada, &list, json!({ "ref": "tmdb:movie:404" }))
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("title-not-found"));
    add(
        &client,
        &ada,
        &list,
        json!({ "ref": "tmdb:movie:13", "after": "tmdb:movie:999" }),
    )
    .await
    .assert_status(StatusCode::NOT_FOUND)
    .assert_problem_type(&support::problem("list-item-not-found"));
    add(
        &client,
        &ada,
        &list,
        json!({ "ref": "tmdb:movie:13", "after": FIGHT_CLUB, "before": REPLY_1988 }),
    )
    .await
    .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
    .assert_problem_type(&support::problem("invalid-placement"));
    assert_eq!(items(&client, &ada, &list).await.len(), 4);

    // A provider outage does not stop a title being added.
    env.metadata.set_all_down(Some(metadata::unavailable()));
    let placeholder: ListItem = add(&client, &ada, &list, json!({ "ref": "tmdb:movie:14" }))
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    env.metadata.set_all_down(None);
    assert_eq!(placeholder.title.reference.to_string(), "tmdb:movie:14");
    // A stored list reads without the provider.
    env.metadata.set_all_down(Some(metadata::unavailable()));
    assert_eq!(items(&client, &ada, &list).await.len(), 5);
    env.metadata.set_all_down(None);
    client
        .delete(&format!("/api/v1/lists/{}/items/tmdb:movie:14", list.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // Paged in order, every item once.
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let url = match &cursor {
            Some(c) => format!("/api/v1/lists/{}/items?limit=3&cursor={c}", list.id),
            None => format!("/api/v1/lists/{}/items?limit=3", list.id),
        };
        let page: ListItemPage = client
            .get(&url)
            .bearer(&ada.read)
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert!(page.degraded.is_empty());
        seen.extend(refs(&page.items));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(seen, [FIGHT_CLUB, ONE_PIECE, BREAKING_BAD, REPLY_1988]);
    client
        .get(&format!("/api/v1/lists/{}/items?cursor=MQ", list.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));

    // Moves: to the start, after a title, before one, to the end, and
    // back and forth many times without drift.
    let moved: ListItem = patch_item(&client, &ada, &list, REPLY_1988, json!({ "after": null }))
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(moved.title.reference.to_string(), REPLY_1988);
    assert_eq!(
        refs(&items(&client, &ada, &list).await),
        [REPLY_1988, FIGHT_CLUB, ONE_PIECE, BREAKING_BAD]
    );
    let noted: ListItem = patch_item(&client, &ada, &list, ONE_PIECE, json!({ "before": null }))
        .await
        .assert_status(StatusCode::OK)
        .json();
    // A move leaves the notes alone.
    assert_eq!(noted.notes.as_deref(), Some("long"));
    assert_eq!(
        refs(&items(&client, &ada, &list).await),
        [REPLY_1988, FIGHT_CLUB, BREAKING_BAD, ONE_PIECE]
    );
    for _ in 0..40 {
        patch_item(
            &client,
            &ada,
            &list,
            ONE_PIECE,
            json!({ "after": REPLY_1988, "before": FIGHT_CLUB }),
        )
        .await
        .assert_status(StatusCode::OK);
        patch_item(
            &client,
            &ada,
            &list,
            ONE_PIECE,
            json!({ "before": FIGHT_CLUB }),
        )
        .await
        .assert_status(StatusCode::OK);
        patch_item(
            &client,
            &ada,
            &list,
            ONE_PIECE,
            json!({ "after": BREAKING_BAD }),
        )
        .await
        .assert_status(StatusCode::OK);
    }
    let settled = items(&client, &ada, &list).await;
    assert_eq!(
        refs(&settled),
        [REPLY_1988, FIGHT_CLUB, BREAKING_BAD, ONE_PIECE]
    );
    assert!(settled.windows(2).all(|w| w[0].position < w[1].position));

    // Notes merge: a value sets them, a move or `{}` leaves them, `null`
    // clears them and leaves the position.
    let noted: ListItem = patch_item(
        &client,
        &ada,
        &list,
        FIGHT_CLUB,
        json!({ "notes": "rewatch" }),
    )
    .await
    .assert_status(StatusCode::OK)
    .json();
    assert_eq!(noted.notes.as_deref(), Some("rewatch"));
    let same: ListItem = patch_item(&client, &ada, &list, FIGHT_CLUB, json!({}))
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(same, noted);
    let cleared: ListItem = patch_item(&client, &ada, &list, FIGHT_CLUB, json!({ "notes": null }))
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(cleared.notes, None);
    assert_eq!(cleared.position, noted.position);

    // Items and anchors outside the list, and placing an item by itself.
    for (reference, body) in [
        ("tmdb:movie:999", json!({ "notes": "x" })),
        (FIGHT_CLUB, json!({ "after": "tmdb:movie:999" })),
        (FIGHT_CLUB, json!({ "before": "tmdb:movie:13" })),
    ] {
        patch_item(&client, &ada, &list, reference, body)
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("list-item-not-found"));
    }
    for body in [
        json!({ "after": FIGHT_CLUB }),
        json!({ "after": REPLY_1988, "before": ONE_PIECE }),
    ] {
        patch_item(&client, &ada, &list, FIGHT_CLUB, body)
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-placement"));
    }

    // Removal, once.
    let url = format!("/api/v1/lists/{}/items/{BREAKING_BAD}", list.id);
    client
        .delete(&url)
        .bearer(&ada.write)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    for target in [
        url,
        format!("/api/v1/lists/{}/items/tmdb:movie:999", list.id),
    ] {
        client
            .delete(&target)
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("list-item-not-found"));
    }
    let after: List = client
        .get(&format!("/api/v1/lists/{}", list.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(after.item_count, Some(3));

    client.assert_conformance();
}

/// A smart list's titles come from its filter, evaluated on every read.
async fn smart_lists(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;

    let smart = create(
        &client,
        &ada,
        json!({ "name": "Good shows", "filter": { "kinds": ["show"], "rating_min": 8, "sort": "top_rated" } }),
    )
    .await;
    assert!(smart.smart);
    assert_eq!(smart.item_count, None);
    let filter = smart.filter.clone().expect("a smart list's filter");
    assert_eq!(filter.kinds, Some(vec![Kind::Show]));
    assert_eq!(filter.rating_min, Some(8));
    assert_eq!(filter.sort, Some(api::lists::SmartSort::TopRated));

    // Each title once, in feed order, with no position.
    let titles = items(&client, &ada, &smart).await;
    assert_eq!(refs(&titles), [BREAKING_BAD, REPLY_1988]);
    assert!(
        titles
            .iter()
            .all(|t| t.position.is_none() && t.added_at.is_none())
    );

    // Paged by offset; a manual list's cursor is not one of its.
    let first: ListItemPage = client
        .get(&format!("/api/v1/lists/{}/items?limit=1", smart.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(refs(&first.items), [BREAKING_BAD]);
    let next = first.next_cursor.expect("a second page");
    let second: ListItemPage = client
        .get(&format!(
            "/api/v1/lists/{}/items?limit=1&cursor={next}",
            smart.id
        ))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(refs(&second.items), [REPLY_1988]);
    assert_eq!(second.next_cursor, None);
    client
        .get(&format!(
            "/api/v1/lists/{}/items?cursor={}",
            smart.id,
            api::page::encode(&"V")
        ))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // Blocked titles drop out, and come back when unblocked.
    client
        .put(&format!("/api/v1/blocklist/{BREAKING_BAD}"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert_eq!(refs(&items(&client, &ada, &smart).await), [REPLY_1988]);
    client
        .delete(&format!("/api/v1/blocklist/{BREAKING_BAD}"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);

    // Merge: a new filter replaces the old; `null` clears its criteria; a
    // rename leaves the filter.
    let url = format!("/api/v1/lists/{}", smart.id);
    let comedies: List = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "filter": { "genres": ["comedy"] } }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        comedies.filter.as_ref().and_then(|f| f.genres.clone()),
        Some(vec!["comedy".to_owned()])
    );
    assert!(items(&client, &ada, &smart).await.is_empty());
    let renamed: List = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "name": "Comedies" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(renamed.filter, comedies.filter);
    let everything: List = client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "filter": null }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(everything.filter, Some(api::lists::ListFilter::default()));
    assert_eq!(
        refs(&items(&client, &ada, &smart).await),
        [FIGHT_CLUB, BREAKING_BAD, REPLY_1988]
    );

    // Only titles in the library with a status, when one is asked for.
    client
        .patch(&url)
        .session(&ada)
        .json(&json!({ "filter": { "statuses": ["watching"] } }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    assert!(items(&client, &ada, &smart).await.is_empty());

    // An unusable filter, at creation or later.
    for filter in [
        json!({ "year_min": 2010, "year_max": 2000 }),
        json!({ "rating_min": 9, "rating_max": 3 }),
    ] {
        client
            .post("/api/v1/lists")
            .session(&ada)
            .json(&json!({ "name": "Bad", "filter": filter }))
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-list"));
        client
            .patch(&url)
            .session(&ada)
            .json(&json!({ "filter": filter }))
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-list"));
    }

    // Nothing is added to a smart list by hand.
    add(&client, &ada, &smart, json!({ "ref": FIGHT_CLUB }))
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .assert_problem_type(&support::problem("not-a-manual-list"));

    // Evaluation calls the provider, and fails the way a feed does.
    for (kind, status, slug) in [
        (
            metadata::unavailable(),
            StatusCode::SERVICE_UNAVAILABLE,
            "upstream-unavailable",
        ),
        (
            metadata::invalid_response(),
            StatusCode::BAD_GATEWAY,
            "upstream-invalid-response",
        ),
    ] {
        env.metadata.set_all_down(Some(kind));
        client
            .get(&format!("/api/v1/lists/{}/items", smart.id))
            .session(&ada)
            .send()
            .await
            .assert_status(status)
            .assert_problem_type(&support::problem(slug));
    }
    env.metadata.set_all_down(None);

    client.assert_conformance();
}

#[tokio::test]
async fn lists_are_created_merged_trashed_and_restored() {
    let env = env!();
    list_lifecycle(&env).await;
}

#[tokio::test]
async fn manual_lists_keep_titles_in_the_order_asked() {
    let env = env!();
    manual_items(&env).await;
}

#[tokio::test]
async fn smart_lists_are_filled_by_their_filter() {
    let env = env!();
    smart_lists(&env).await;
}

#[tokio::test]
async fn smart_lists_draw_on_the_library() {
    let env = env!();
    smart::library_smart_lists(&env).await;
}

#[tokio::test]
async fn lists_cover_every_declared_response() {
    let env = env!();
    list_lifecycle(&env).await;
    manual_items(&env).await;
    smart_lists(&env).await;
    smart::library_smart_lists(&env).await;
    support::sweep::sweep(&env, "lists").await;
    env.assert_covered("lists");
}
