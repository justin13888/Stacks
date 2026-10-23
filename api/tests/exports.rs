//! Exporting the library as a JSON backup and as CSV.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use api::catalog::Status;
use api::exports::Backup;
use kynos::http::StatusCode;
use stacks_core::Patch;
use stacks_core::entity::enums::{DatePrecision, EntryStatus, Role};
use stacks_core::entity::media;
use stacks_core::library::{EntryPatch, LibraryService, RunDate, RunPatch};
use stacks_core::media::MediaService;
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, ONE_PIECE};
use support::{Credentials, Env, TestUser};

/// Track a title through core, as the library area would.
async fn track(env: &Env, user: &TestUser, raw: &str, patch: EntryPatch) -> media::Model {
    let media = MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()))
        .ensure_in_catalog(&metadata::id(raw))
        .await
        .unwrap();
    LibraryService::new(env.db.db.clone())
        .upsert(user.id(), media.id, patch)
        .await
        .unwrap();
    media
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[tokio::test]
async fn the_backup_and_the_csv_hold_what_the_library_holds() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let eve = env.user(Role::User).await;
    let library = LibraryService::new(env.db.db.clone());

    let fight_club = track(
        &env,
        &ada,
        FIGHT_CLUB,
        EntryPatch {
            status: Some(EntryStatus::Completed),
            score: Patch::Value(90),
            ..Default::default()
        },
    )
    .await;
    let run = library
        .runs(ada.id(), fight_club.id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("completing opens a run");
    let started = chrono::NaiveDate::from_ymd_opt(2024, 3, 1).unwrap();
    library
        .update_run(
            ada.id(),
            run.id,
            RunPatch {
                started: Patch::Value(RunDate {
                    date: started,
                    precision: DatePrecision::Day,
                }),
                completed: Patch::Absent,
            },
        )
        .await
        .unwrap();
    track(&env, &ada, BREAKING_BAD, EntryPatch::default()).await;
    track(&env, &eve, ONE_PIECE, EntryPatch::default()).await;

    let response = client
        .get("/api/v1/exports/backup")
        .bearer(&ada.read)
        .send()
        .await;
    response.assert_status(StatusCode::OK);
    let disposition = response.header("content-disposition").expect("a filename");
    assert!(disposition.starts_with("attachment"), "{disposition}");
    assert!(
        disposition.contains(&format!("stacks-backup-{}.json", today())),
        "{disposition}"
    );
    let backup: Backup = response.json();
    assert_eq!(backup.version, 1);

    // Exactly the caller's live entries, as the library holds them.
    let held = library.all(ada.id()).await.unwrap();
    assert_eq!(backup.entries.len(), held.len());
    let by_ref = |raw: &str| {
        backup
            .entries
            .iter()
            .find(|e| e.title.to_string() == raw)
            .unwrap_or_else(|| panic!("{raw} is in the backup"))
    };
    let movie = by_ref(FIGHT_CLUB);
    assert_eq!(movie.status, Status::Completed);
    assert_eq!(movie.score, Some(90));
    assert_eq!(movie.started_on, Some(started));
    assert_eq!(movie.completed_on, run.completed_on);
    let show = by_ref(BREAKING_BAD);
    assert_eq!(show.status, Status::Planning);
    assert_eq!(show.score, None);
    assert!(
        backup
            .entries
            .iter()
            .all(|e| e.title.to_string() != ONE_PIECE)
    );

    // A deleted entry leaves the backup.
    library.delete(ada.id(), fight_club.id).await.unwrap();
    let after: Backup = client
        .get("/api/v1/exports/backup")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(after.entries.len(), 1);
    library.restore(ada.id(), fight_club.id).await.unwrap();

    let csv = client.get("/api/v1/exports/csv").session(&ada).send().await;
    csv.assert_status(StatusCode::OK);
    assert_eq!(csv.header("content-type"), Some("text/csv; charset=utf-8"));
    assert!(
        csv.header("content-disposition")
            .unwrap()
            .contains(&format!("stacks-library-{}.csv", today()))
    );
    let text = csv.text();
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(
            "source,mal_id,tmdb_id,imdb_id,anilist_id,tvdb_id,status,score,progress,started_at,completed_at"
        )
    );
    let rows: Vec<&str> = lines.collect();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|r| r.starts_with("tmdb:movie,,550,,,,completed,90,"))
    );
    assert!(
        rows.iter()
            .any(|r| r.starts_with("tmdb:show,,1396,,,,planning,,"))
    );

    // An empty library is an empty export, not an error.
    let empty: Backup = client
        .get("/api/v1/exports/backup")
        .session(&env.user(Role::User).await)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(empty.entries.is_empty());

    client.assert_conformance();
    support::sweep::sweep(&env, "exports").await;
    env.assert_covered("exports");
}
