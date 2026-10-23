//! The caller's statistics.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use api::catalog::{Category, Status};
use api::stats::{CategoryCount, GenreCount, MonthCount, Stats, StatusCount, YearCount};
use chrono::{Datelike, TimeZone};
use kynos::http::StatusCode;
use stacks_core::entity::enums::{EntryStatus, PlaySource, Role};
use stacks_core::library::{EntryPatch, LibraryService, LogPlay};
use stacks_core::media::MediaService;
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, REPLY_1988};
use support::{Credentials, Env, TestUser};

async fn stats(env: &Env, user: &TestUser, query: &str) -> Stats {
    let client = env.client();
    let stats = client
        .get(&format!("/api/v1/stats{query}"))
        .session(user)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    client.assert_conformance();
    stats
}

fn watched(year: i32, month: u32, day: u32) -> LogPlay {
    LogPlay {
        episode: None,
        watched_at: Some(
            chrono::Utc
                .with_ymd_and_hms(year, month, day, 20, 0, 0)
                .unwrap()
                .fixed_offset(),
        ),
        source: PlaySource::Manual,
        import_id: None,
    }
}

#[tokio::test]
async fn stats_count_the_library_and_limit_activity_to_a_year() {
    let env = env!();
    let ada = env.user(Role::User).await;
    let catalog = MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()));
    let library = LibraryService::new(env.db.db.clone());
    let this_year = chrono::Utc::now().year();

    // Nothing yet: zeros and empty lists, not an error.
    let empty = stats(&env, &ada, "").await;
    assert_eq!(empty.watch_time_minutes, 0);
    assert!(empty.by_status.is_empty() && empty.top_genres.is_empty());

    // Fight Club watched in March 2024 (a movie play completes it, today);
    // two episodes of Breaking Bad, one in 2024 and one in 2025; Reply 1988
    // planned.
    let fight_club = catalog
        .ensure_in_catalog(&metadata::id(FIGHT_CLUB))
        .await
        .unwrap();
    library
        .log_play(ada.id(), &fight_club, watched(2024, 3, 10))
        .await
        .unwrap();
    let breaking_bad = catalog
        .ensure_in_catalog(&metadata::id(BREAKING_BAD))
        .await
        .unwrap();
    for (date, episode) in [((2024, 3, 11), 1), ((2025, 1, 2), 2)] {
        let mut log = watched(date.0, date.1, date.2);
        log.episode = Some((1, episode));
        library
            .log_play(ada.id(), &breaking_bad, log)
            .await
            .unwrap();
    }
    let reply = catalog
        .ensure_in_catalog(&metadata::id(REPLY_1988))
        .await
        .unwrap();
    library
        .upsert(
            ada.id(),
            reply.id,
            EntryPatch {
                status: Some(EntryStatus::Planning),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let all = stats(&env, &ada, "").await;
    assert_eq!(all.year, None);
    assert_eq!(
        all.by_status,
        [
            StatusCount {
                status: Status::Planning,
                count: 1
            },
            StatusCount {
                status: Status::Watching,
                count: 1
            },
            StatusCount {
                status: Status::Completed,
                count: 1
            },
        ]
    );
    assert_eq!(
        all.by_category,
        [
            CategoryCount {
                category: Category::Movie,
                count: 1
            },
            CategoryCount {
                category: Category::Series,
                count: 1
            },
            CategoryCount {
                category: Category::KDrama,
                count: 1
            },
        ]
    );
    // The movie's 139 minutes and two 47-minute episodes.
    assert_eq!(all.watch_time_minutes, 139 + 47 + 47);
    assert_eq!(
        all.monthly_activity,
        [
            MonthCount {
                month: "2024-03".into(),
                plays: 2
            },
            MonthCount {
                month: "2025-01".into(),
                plays: 1
            },
        ]
    );
    assert_eq!(
        all.completions_by_year,
        [YearCount {
            year: this_year,
            completed: 1
        }]
    );
    assert_eq!(
        all.top_genres,
        [GenreCount {
            genre: "Drama".into(),
            count: 1
        }]
    );

    // A year limits the activity, never the library's counts.
    let in_2024 = stats(&env, &ada, "?year=2024").await;
    assert_eq!(in_2024.year, Some(2024));
    assert_eq!(in_2024.by_status, all.by_status);
    assert_eq!(in_2024.watch_time_minutes, 139 + 47);
    assert_eq!(in_2024.monthly_activity.len(), 1);
    assert!(in_2024.completions_by_year.is_empty());
    assert!(in_2024.top_genres.is_empty());
    let now = stats(&env, &ada, &format!("?year={this_year}")).await;
    assert_eq!(now.completions_by_year, all.completions_by_year);
    assert_eq!(now.top_genres, all.top_genres);
    let quiet = stats(&env, &ada, "?year=1999").await;
    assert_eq!(quiet.watch_time_minutes, 0);
    assert!(quiet.monthly_activity.is_empty());

    // Only the caller's own.
    let eve = env.user(Role::User).await;
    let theirs = stats(&env, &eve, "").await;
    assert!(theirs.by_status.is_empty());

    let client = env.client();
    for bad in ["?year=1800", "?year=3001", "?year=soon"] {
        client
            .get(&format!("/api/v1/stats{bad}"))
            .bearer(&ada.read)
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);
    }
    client.assert_conformance();

    support::sweep::sweep(&env, "stats").await;
    env.assert_covered("stats");
}
