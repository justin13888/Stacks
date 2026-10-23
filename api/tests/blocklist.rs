//! The blocklist, and how it hides titles from discovery.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use api::app::App;
use api::blocklist::BlockedTitle;
use api::catalog::TitlePage;
use api::page::Page;
use kynos::http::StatusCode;
use kynos::test::TestClient;
use stacks_core::entity::enums::Role;
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, ONE_PIECE, REPLY_1988};
use support::{Credentials, Env, TestUser};

async fn titles(client: &TestClient<App>, user: &TestUser, url: &str) -> Vec<String> {
    let page: TitlePage = client
        .get(url)
        .session(user)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    page.items.iter().map(|t| t.reference.to_string()).collect()
}

async fn blocked(client: &TestClient<App>, user: &TestUser, url: &str) -> Page<BlockedTitle> {
    client
        .get(url)
        .bearer(&user.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
}

fn refs(page: &Page<BlockedTitle>) -> Vec<String> {
    page.items
        .iter()
        .map(|b| b.title.reference.to_string())
        .collect()
}

async fn blocking(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let bob = env.user(Role::User).await;
    let search = "/api/v1/search?q=x";
    let feed = "/api/v1/feeds/trending?kind=show";
    let recommendations = format!("/api/v1/titles/{FIGHT_CLUB}/recommendations");
    assert!(
        titles(&client, &ada, search)
            .await
            .contains(&BREAKING_BAD.to_owned())
    );

    // Blocking twice is one block.
    for _ in 0..2 {
        client
            .put(&format!("/api/v1/blocklist/{BREAKING_BAD}"))
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    client
        .put(&format!("/api/v1/blocklist/{ONE_PIECE}"))
        .bearer(&ada.write)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let list = blocked(&client, &ada, "/api/v1/blocklist").await;
    assert_eq!(refs(&list), [ONE_PIECE, BREAKING_BAD]);
    assert_eq!(list.items[1].title.title, "Breaking Bad");

    // Gone from search, feeds and recommendations, for the blocker only.
    assert_eq!(
        titles(&client, &ada, search).await,
        [FIGHT_CLUB, REPLY_1988]
    );
    assert_eq!(titles(&client, &ada, feed).await, [FIGHT_CLUB, REPLY_1988]);
    assert_eq!(
        titles(&client, &ada, &recommendations).await,
        [FIGHT_CLUB, REPLY_1988]
    );
    assert_eq!(
        titles(&client, &bob, search).await,
        [FIGHT_CLUB, BREAKING_BAD, REPLY_1988, ONE_PIECE]
    );
    assert!(
        blocked(&client, &bob, "/api/v1/blocklist")
            .await
            .items
            .is_empty()
    );

    // Paged, newest first.
    let first = blocked(&client, &ada, "/api/v1/blocklist?limit=1").await;
    assert_eq!(refs(&first), [ONE_PIECE]);
    let next = first.next_cursor.clone().expect("a second page");
    let second = blocked(
        &client,
        &ada,
        &format!("/api/v1/blocklist?limit=1&cursor={next}"),
    )
    .await;
    assert_eq!(refs(&second), [BREAKING_BAD]);
    assert_eq!(second.next_cursor, None);
    client
        .get("/api/v1/blocklist?cursor=eA")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));

    // Unblocked, it is back; unblocking twice, or a title never seen, is
    // fine.
    for target in [BREAKING_BAD, BREAKING_BAD, "tmdb:movie:4242"] {
        client
            .delete(&format!("/api/v1/blocklist/{target}"))
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::NO_CONTENT);
    }
    assert_eq!(
        titles(&client, &ada, search).await,
        [FIGHT_CLUB, BREAKING_BAD, REPLY_1988]
    );
    assert_eq!(
        titles(&client, &ada, feed).await,
        [FIGHT_CLUB, BREAKING_BAD, REPLY_1988]
    );
    assert_eq!(
        refs(&blocked(&client, &ada, "/api/v1/blocklist").await),
        [ONE_PIECE]
    );

    // A title the provider does not have cannot be blocked; one whose
    // provider is down is stored as a placeholder and blocked.
    env.metadata.set_missing("tmdb:movie:404");
    client
        .put("/api/v1/blocklist/tmdb:movie:404")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("title-not-found"));
    env.metadata.set_all_down(Some(metadata::unavailable()));
    client
        .put("/api/v1/blocklist/tmdb:movie:77")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let stored = blocked(&client, &ada, "/api/v1/blocklist").await;
    env.metadata.set_all_down(None);
    assert_eq!(refs(&stored), ["tmdb:movie:77", ONE_PIECE]);

    client.assert_conformance();
}

#[tokio::test]
async fn blocked_titles_leave_discovery_until_unblocked() {
    let env = env!();
    blocking(&env).await;
}

#[tokio::test]
async fn blocklist_covers_every_declared_response() {
    let env = env!();
    blocking(&env).await;
    support::sweep::sweep(&env, "blocklist").await;
    env.assert_covered("blocklist");
}
