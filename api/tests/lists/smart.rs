//! Smart lists that draw on the caller's library: its categories, tags and
//! the caller's own scores.

use api::catalog::Category;
use api::lists::{List, SmartSort};
use api::tags::Tag;
use kynos::http::StatusCode;
use serde_json::json;
use stacks_core::entity::enums::Role;

use super::support::metadata::{self, BREAKING_BAD, ONE_PIECE, REPLY_1988};
use super::support::{self, Credentials, Env};
use super::{create, items, refs};

/// "Every K-drama I rated 80 or more", "anime tagged comfort".
pub async fn library_smart_lists(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let bob = env.user(Role::User).await;
    for (reference, body) in [
        (REPLY_1988, json!({ "status": "completed", "score": 85 })),
        (BREAKING_BAD, json!({ "status": "completed", "score": 90 })),
        (ONE_PIECE, json!({ "status": "watching" })),
    ] {
        client
            .put(&format!("/api/v1/library/{reference}"))
            .session(&ada)
            .json(&body)
            .send()
            .await
            .assert_status(StatusCode::CREATED);
    }
    let tag = |name: &'static str, user| {
        let client = &client;
        async move {
            let tag: Tag = client
                .post("/api/v1/tags")
                .session(user)
                .json(&json!({ "name": name }))
                .send()
                .await
                .assert_status(StatusCode::CREATED)
                .json();
            tag
        }
    };
    let comfort = tag("comfort", &ada).await;
    let bobs = tag("bobs", &bob).await;
    client
        .put(&format!("/api/v1/library/{ONE_PIECE}/tags/{}", comfort.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);

    let kdramas = create(
        &client,
        &ada,
        json!({ "name": "K-dramas I loved", "filter": {
            "sort": "library", "categories": ["k-drama"], "score_min": 80,
        } }),
    )
    .await;
    let filter = kdramas.filter.clone().expect("a smart list's filter");
    assert_eq!(filter.sort, Some(SmartSort::Library));
    assert_eq!(filter.categories, Some(vec![Category::KDrama]));
    assert_eq!(filter.score_min, Some(80));
    assert_eq!(refs(&items(&client, &ada, &kdramas).await), [REPLY_1988]);

    // The library is read from the catalog snapshot: no provider is called.
    env.metadata.set_all_down(Some(metadata::unavailable()));
    assert_eq!(refs(&items(&client, &ada, &kdramas).await), [REPLY_1988]);
    let comfy = create(
        &client,
        &ada,
        json!({ "name": "Comfort anime", "filter": {
            "sort": "library", "categories": ["anime"], "tags": [comfort.id],
        } }),
    )
    .await;
    assert_eq!(comfy.filter.and_then(|f| f.tags), Some(vec![comfort.id]));
    let comfy: List = client
        .get(&format!("/api/v1/lists/{}", comfy.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(refs(&items(&client, &ada, &comfy).await), [ONE_PIECE]);
    env.metadata.set_all_down(None);

    // From a feed, the caller's score narrows it too; an untracked title
    // has none.
    let loved = create(
        &client,
        &ada,
        json!({ "name": "Loved", "filter": { "score_min": 86 } }),
    )
    .await;
    assert_eq!(refs(&items(&client, &ada, &loved).await), [BREAKING_BAD]);

    // A tag must be the caller's; a score is 0 to 100, its bounds in order.
    client
        .post("/api/v1/lists")
        .session(&ada)
        .json(&json!({ "name": "Bad", "filter": { "score_min": 101 } }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    for filter in [
        json!({ "tags": [bobs.id] }),
        json!({ "score_min": 90, "score_max": 80 }),
    ] {
        client
            .post("/api/v1/lists")
            .session(&ada)
            .json(&json!({ "name": "Bad", "filter": filter }))
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("invalid-list"));
    }

    client.assert_conformance();
}
