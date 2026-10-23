//! Browsing the catalog, and how it degrades when providers fail.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use api::catalog::{Category, Kind, SeasonDetail, Seasons, TitleDetail, TitlePage};
use kynos::http::StatusCode;
use stacks_core::entity::enums::Role;
use stacks_core::media::{MediaService, MetadataErrorKind, Provider};
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, ONE_PIECE, REPLY_1988};
use support::{Credentials, Env};

async fn page(
    client: &kynos::test::TestClient<api::app::App>,
    user: &support::TestUser,
    url: &str,
) -> TitlePage {
    client
        .get(url)
        .session(user)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
}

fn refs(page: &TitlePage) -> Vec<String> {
    page.items.iter().map(|t| t.reference.to_string()).collect()
}

/// Put a title in the catalog snapshot, as tracking it would.
async fn snapshot(env: &Env, raw: &str) {
    MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()))
        .ensure_in_catalog(&metadata::id(raw))
        .await
        .expect("a snapshot");
}

#[tokio::test]
async fn search_merges_providers_filters_and_pages() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;

    let all = page(&client, &ada, "/api/v1/search?q=bad").await;
    assert_eq!(
        refs(&all),
        [FIGHT_CLUB, BREAKING_BAD, REPLY_1988, ONE_PIECE]
    );
    assert!(all.degraded.is_empty());
    let next = all.next_cursor.clone().expect("a second page");
    let second = page(
        &client,
        &ada,
        &format!("/api/v1/search?q=bad&cursor={next}"),
    )
    .await;
    assert_eq!(second.next_cursor, None);

    let shows = page(&client, &ada, "/api/v1/search?q=bad&kind=show").await;
    assert!(shows.items.iter().all(|t| t.kind == Kind::Show));
    let kdramas = page(&client, &ada, "/api/v1/search?q=bad&category=k-drama").await;
    assert_eq!(refs(&kdramas), [REPLY_1988]);
    assert_eq!(kdramas.items[0].category, Category::KDrama);
    let anime = page(&client, &ada, "/api/v1/search?q=bad&category=anime").await;
    assert_eq!(refs(&anime), [ONE_PIECE]);

    // A cursor the server never issued is refused, not silently page one.
    client
        .get("/api/v1/search?q=bad&cursor=bm90LWEtcGFnZQ")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));
    client
        .get("/api/v1/search?q=bad&kind=book")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // One provider down: the other's results, and the failure named.
    env.metadata.set_down(
        Provider::AniList,
        Some(MetadataErrorKind::RateLimited { retry_after: None }),
    );
    let partial = page(&client, &ada, "/api/v1/search?q=bad").await;
    assert_eq!(refs(&partial), [FIGHT_CLUB, BREAKING_BAD, REPLY_1988]);
    assert_eq!(partial.degraded.len(), 1);
    assert_eq!(partial.degraded[0].class, "rate_limited");

    // Both down: a 503 that says when to come back.
    env.metadata.set_down(
        Provider::Tmdb,
        Some(MetadataErrorKind::RateLimited {
            retry_after: Some(std::time::Duration::from_secs(12)),
        }),
    );
    let limited = client
        .get("/api/v1/search?q=bad")
        .session(&ada)
        .send()
        .await;
    limited
        .assert_status(StatusCode::SERVICE_UNAVAILABLE)
        .assert_problem_type(&support::problem("upstream-rate-limited"));
    assert_eq!(limited.header("retry-after"), Some("12"));
    env.metadata
        .set_all_down(Some(metadata::invalid_response()));
    client
        .get("/api/v1/search?q=bad")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_GATEWAY)
        .assert_problem_type(&support::problem("upstream-invalid-response"));
    env.metadata.set_all_down(None);

    client.assert_conformance();
}

#[tokio::test]
async fn a_title_falls_back_to_its_snapshot_when_the_provider_fails() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;

    let live: TitleDetail = client
        .get(&format!("/api/v1/titles/{BREAKING_BAD}"))
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(live.summary.title, "Breaking Bad");
    assert!(!live.freshness.stale);
    assert_eq!(live.season_count, Some(2));

    // Untracked and the provider down: nothing to serve.
    env.metadata.set_all_down(Some(metadata::unavailable()));
    client
        .get(&format!("/api/v1/titles/{BREAKING_BAD}"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::SERVICE_UNAVAILABLE)
        .assert_problem_type(&support::problem("upstream-unavailable"));
    // Tracked and the provider down: the snapshot, marked stale.
    env.metadata.set_all_down(None);
    snapshot(&env, BREAKING_BAD).await;
    env.metadata.set_all_down(Some(metadata::unavailable()));
    let stale: TitleDetail = client
        .get(&format!("/api/v1/titles/{BREAKING_BAD}"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(stale.freshness.stale);
    assert_eq!(stale.summary.title, "Breaking Bad");
    let seasons: Seasons = client
        .get(&format!("/api/v1/titles/{BREAKING_BAD}/seasons"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(seasons.freshness.stale);
    assert_eq!(seasons.items.len(), 2);
    env.metadata.set_all_down(None);

    // A title the provider does not have.
    env.metadata.set_missing("tmdb:movie:999999");
    client
        .get("/api/v1/titles/tmdb:movie:999999")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("title-not-found"));
    client
        .get("/api/v1/titles/imdb:movie:1")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    client.assert_conformance();
}

#[tokio::test]
async fn catalog_reads_cover_every_declared_response() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;

    // Seasons and episodes of a show.
    let seasons: Seasons = client
        .get(&format!("/api/v1/titles/{BREAKING_BAD}/seasons"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        seasons.items.iter().map(|s| s.number).collect::<Vec<_>>(),
        [1, 2]
    );
    let season: SeasonDetail = client
        .get(&format!("/api/v1/titles/{BREAKING_BAD}/seasons/1"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(season.episodes.len(), 3);
    assert_eq!(
        season.episodes[0].air_date,
        chrono::NaiveDate::from_ymd_opt(2020, 1, 6)
    );
    for missing in [
        format!("/api/v1/titles/{FIGHT_CLUB}/seasons"),
        format!("/api/v1/titles/{BREAKING_BAD}/seasons/9"),
        format!("/api/v1/titles/{ONE_PIECE}/seasons/1"),
    ] {
        client
            .get(&missing)
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
    }

    // Recommendations and feeds.
    let recs = page(
        &client,
        &ada,
        &format!("/api/v1/titles/{FIGHT_CLUB}/recommendations"),
    )
    .await;
    assert!(!recs.items.is_empty());
    let next = recs.next_cursor.clone().expect("a second page");
    page(
        &client,
        &ada,
        &format!("/api/v1/titles/{FIGHT_CLUB}/recommendations?cursor={next}"),
    )
    .await;
    for feed in [
        "trending",
        "popular",
        "top-rated",
        "trending-anime",
        "popular-anime",
        "top-rated-anime",
    ] {
        page(&client, &ada, &format!("/api/v1/feeds/{feed}")).await;
    }
    page(&client, &ada, "/api/v1/feeds/trending?kind=show").await;
    client
        .get("/api/v1/feeds/everything")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    client
        .get("/api/v1/feeds/trending?cursor=AA")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    env.metadata.set_missing("tmdb:movie:999999");
    client
        .get("/api/v1/titles/tmdb:movie:999999")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);

    // Every live read in each upstream failure class.
    let reads = [
        "/api/v1/search?q=x".to_owned(),
        format!("/api/v1/titles/{FIGHT_CLUB}"),
        format!("/api/v1/titles/{BREAKING_BAD}/seasons"),
        format!("/api/v1/titles/{BREAKING_BAD}/seasons/1"),
        format!("/api/v1/titles/{FIGHT_CLUB}/recommendations"),
        "/api/v1/feeds/trending".to_owned(),
    ];
    for (kind, status) in [
        (metadata::unavailable(), StatusCode::SERVICE_UNAVAILABLE),
        (metadata::invalid_response(), StatusCode::BAD_GATEWAY),
    ] {
        env.metadata.set_all_down(Some(kind));
        for read in &reads {
            client
                .get(read)
                .session(&ada)
                .send()
                .await
                .assert_status(status);
        }
    }
    env.metadata.set_all_down(None);

    // Recommendations of a title the provider cannot recommend from.
    client
        .get(&format!(
            "/api/v1/titles/{FIGHT_CLUB}/recommendations?cursor=AA"
        ))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    env.metadata
        .set_down(Provider::Tmdb, Some(MetadataErrorKind::NotFound));
    client
        .get(&format!("/api/v1/titles/{FIGHT_CLUB}/recommendations"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND);
    env.metadata.set_all_down(None);

    // A failing database is a 500 on every read, even with providers up.
    let degraded = env.degraded_client(support::db::Credential::Session).await;
    for read in [
        "/api/v1/search?q=x".to_owned(),
        format!("/api/v1/titles/{FIGHT_CLUB}"),
        format!("/api/v1/titles/{BREAKING_BAD}/seasons"),
        format!("/api/v1/titles/{BREAKING_BAD}/seasons/1"),
        format!("/api/v1/titles/{FIGHT_CLUB}/recommendations"),
        "/api/v1/feeds/trending".to_owned(),
    ] {
        degraded
            .get(&read)
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    }
    degraded.assert_conformance();

    client.assert_conformance();
    support::sweep::sweep(&env, "catalog").await;
    env.assert_covered("catalog");
}
