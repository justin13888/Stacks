//! An AniList show's episodes are tracked one by one, as a TMDB show's are:
//! plays of single episodes, marking up to an episode, progress, Up Next and
//! stats, all over the show's one synthesised season.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use api::catalog::Status;
use api::library::{LibraryEntryDetail, LoggedPlays, Play, UpNextItem};
use api::page::Page;
use api::stats::Stats;
use kynos::http::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use stacks_core::entity::enums::Role;
use stacks_core::media::Provider;
use support::metadata::{self, ANIME_EPISODES};
use support::{Credentials, TestUser};

type Client = kynos::test::TestClient<api::app::App>;

/// AniList shows of this suite's own, so no other suite's catalog rows
/// change what it sees.
const FRIEREN: &str = "anilist:show:154587";
const LONG_RUN: &str = "anilist:show:8000";

async fn get<T: DeserializeOwned>(client: &Client, user: &TestUser, url: &str) -> T {
    client
        .get(url)
        .session(user)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
}

async fn plays(client: &Client, user: &TestUser, reference: &str, body: Value) -> Vec<Play> {
    let logged: LoggedPlays = client
        .post(&format!("/api/v1/library/{reference}/plays"))
        .session(user)
        .json(&body)
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    logged.plays
}

fn episodes(plays: &[Play]) -> Vec<(Option<u32>, Option<u32>)> {
    plays.iter().map(|p| (p.season, p.episode)).collect()
}

async fn entry(client: &Client, user: &TestUser, reference: &str) -> LibraryEntryDetail {
    get(client, user, &format!("/api/v1/library/{reference}")).await
}

#[tokio::test]
async fn an_anilist_show_is_tracked_episode_by_episode() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;

    // One episode, then marking up to a later one.
    let first = plays(
        &client,
        &ada,
        FRIEREN,
        json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": 1 }] }),
    )
    .await;
    assert_eq!(episodes(&first), [(Some(1), Some(1))]);
    let detail = entry(&client, &ada, FRIEREN).await;
    assert_eq!(detail.entry.status, Status::Watching);
    assert_eq!(detail.entry.progress.current_run_episodes, 1);

    let through = plays(
        &client,
        &ada,
        FRIEREN,
        json!({ "kind": "through", "season": 1, "episode": 4 }),
    )
    .await;
    assert_eq!(
        episodes(&through),
        [(Some(1), Some(2)), (Some(1), Some(3)), (Some(1), Some(4))],
        "marking up to S1E4 logs only the episodes not yet watched"
    );
    let detail = entry(&client, &ada, FRIEREN).await;
    assert_eq!(detail.entry.progress.current_run_episodes, 4);
    assert_eq!(detail.entry.progress.plays, 4);

    // Up Next is the episode after the furthest watched.
    let next: Page<UpNextItem> = get(&client, &ada, "/api/v1/up-next").await;
    let anime: Vec<(u32, u32)> = next
        .items
        .iter()
        .filter(|i| i.title.reference.to_string() == FRIEREN)
        .map(|i| (i.season, i.episode))
        .collect();
    assert_eq!(anime, [(1, 5)]);

    // Stats count the plays, at the title's runtime per episode.
    let stats: Stats = get(&client, &ada, "/api/v1/stats").await;
    assert_eq!(stats.episode_plays, 4);
    assert_eq!(stats.watch_time_minutes, 4 * 47);

    // Marking up to the last episode leaves nothing up next.
    let last = i64::from(ANIME_EPISODES);
    plays(
        &client,
        &ada,
        FRIEREN,
        json!({ "kind": "through", "season": 1, "episode": last }),
    )
    .await;
    let detail = entry(&client, &ada, FRIEREN).await;
    assert_eq!(
        detail.entry.progress.current_run_episodes,
        u64::from(ANIME_EPISODES)
    );
    let next: Page<UpNextItem> = get(&client, &ada, "/api/v1/up-next").await;
    assert!(
        next.items
            .iter()
            .all(|i| i.title.reference.to_string() != FRIEREN)
    );

    // Past the total, and any other season, is no such episode.
    for body in [
        json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": last + 1 }] }),
        json!({ "kind": "episodes", "episodes": [{ "season": 2, "episode": 1 }] }),
        json!({ "kind": "through", "season": 2, "episode": 1 }),
    ] {
        client
            .post(&format!("/api/v1/library/{FRIEREN}/plays"))
            .session(&ada)
            .json(&body)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("episode-not-found"));
    }
    client.assert_conformance();
}

#[tokio::test]
async fn an_anilist_show_logs_episodes_through_an_outage() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;

    // Tracking the show stores its one season's count; then AniList fails.
    client
        .put(&format!("/api/v1/library/{LONG_RUN}"))
        .session(&ada)
        .json(&json!({ "status": "watching" }))
        .send()
        .await
        .assert_status(StatusCode::CREATED);
    env.metadata
        .set_down(Provider::AniList, Some(metadata::unavailable()));

    let through = plays(
        &client,
        &ada,
        LONG_RUN,
        json!({ "kind": "through", "season": 1, "episode": 3 }),
    )
    .await;
    assert_eq!(
        episodes(&through),
        [(Some(1), Some(1)), (Some(1), Some(2)), (Some(1), Some(3))]
    );
    client
        .post(&format!("/api/v1/library/{LONG_RUN}/plays"))
        .session(&ada)
        .json(&json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": 99 }] }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("episode-not-found"));
    env.metadata.set_down(Provider::AniList, None);
    client.assert_conformance();
}
