//! An upstream failure blocks a play only when nothing is known of the
//! show's seasons.

use std::sync::Arc;

use super::support::{Credentials, Env, metadata};
use super::{entry_url, plays};
use kynos::http::StatusCode;
use serde_json::json;
use stacks_core::entity::enums::Role;
use stacks_core::media::MediaService;

pub(super) async fn plays_through_an_outage(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    for (kind, status, reference) in [
        (
            metadata::unavailable(),
            StatusCode::SERVICE_UNAVAILABLE,
            "tmdb:show:5000",
        ),
        (
            metadata::invalid_response(),
            StatusCode::BAD_GATEWAY,
            "tmdb:show:5001",
        ),
    ] {
        env.metadata.set_all_down(Some(kind));
        client
            .post(&format!("{}/plays", entry_url(reference)))
            .session(&ada)
            .json(&json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": 1 }] }))
            .send()
            .await
            .assert_status(status);
    }
    env.metadata.set_all_down(None);
    // Once the provider is back, the placeholder's seasons are fetched.
    let logged = plays(
        &client,
        &ada,
        "tmdb:show:5000",
        json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": 1 }] }),
    )
    .await;
    assert_eq!(logged.len(), 1);
    // With the seasons known, an outage no longer blocks a play.
    MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()))
        .get_seasons(&metadata::id("tmdb:show:5000"))
        .await
        .unwrap();
    env.metadata.set_all_down(Some(metadata::unavailable()));
    let logged = plays(
        &client,
        &ada,
        "tmdb:show:5000",
        json!({ "kind": "through", "season": 2, "episode": 2 }),
    )
    .await;
    assert_eq!(
        logged
            .iter()
            .map(|p| (p.season, p.episode))
            .collect::<Vec<_>>(),
        [
            (Some(1), Some(2)),
            (Some(1), Some(3)),
            (Some(2), Some(1)),
            (Some(2), Some(2))
        ]
    );
    env.metadata.set_all_down(None);
    client.assert_conformance();
}
