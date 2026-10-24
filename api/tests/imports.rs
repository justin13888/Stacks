//! Uploading an export, following its import, and the upload's refusals.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use api::app::App;
use api::catalog::Status;
use api::imports::{
    ConflictStrategy, Import, ImportDetail, ImportItem, ImportOutcome, ImportStatus, PreviousEntry,
    ResolvedImportItem, RestoreAction, RestoreMode, RestoreReport,
};
use api::page::Page;
use kynos::http::StatusCode;
use kynos::test::{TestClient, TestRequest};
use serde_json::json;
use stacks_core::entity::enums::Role;
use stacks_core::jobs::{JobContext, JobRunner};
use stacks_core::library::LibraryService;
use support::metadata::{BREAKING_BAD, FIGHT_CLUB, REPLY_1988};
use support::{Credentials, Env, TestUser};
use uuid::Uuid;

const BOUNDARY: &str = "stacks-test-boundary";

/// A TMDB export: Fight Club, which the mock provider has, and a movie it
/// does not.
const TMDB_EXPORT: &str = r#"[
    {"tmdb_id": 550, "title": "Fight Club", "media_type": "movie", "status": "Completed", "rating": 9},
    {"tmdb_id": 999999, "title": "Lost", "media_type": "movie", "status": "Plan to Watch"}
]"#;

/// A `multipart/form-data` body: the text fields, then the file.
fn form(fields: &[(&str, &str)], file: Option<(&str, &[u8])>) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    if let Some((filename, bytes)) = file {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/json\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn multipart() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

fn upload<'c>(
    client: &'c TestClient<App>,
    user: &TestUser,
    fields: &[(&str, &str)],
    file: Option<(&str, &[u8])>,
) -> TestRequest<'c, App> {
    client
        .post("/api/v1/imports")
        .session(user)
        .body(&multipart(), form(fields, file))
}

/// Run the background worker once, as the job runner would.
async fn work(env: &Env) -> u64 {
    JobRunner::new(JobContext {
        db: env.db.db.clone(),
        metadata: Arc::new(env.metadata.clone()),
    })
    .run_once()
    .await
    .unwrap()
}

#[tokio::test]
async fn an_upload_is_queued_imported_and_followed_row_by_row() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let eve = env.user(Role::User).await;
    env.metadata.set_missing("tmdb:movie:999999");

    let accepted = upload(
        &client,
        &ada,
        &[("source", "tmdb_json"), ("conflict_strategy", "overwrite")],
        Some(("watchlist.json", TMDB_EXPORT.as_bytes())),
    )
    .send()
    .await;
    accepted.assert_status(StatusCode::ACCEPTED);
    let queued: Import = accepted.json();
    assert_eq!(
        accepted.header("location"),
        Some(format!("/api/v1/imports/{}", queued.id).as_str())
    );
    assert_eq!(queued.status, ImportStatus::Pending);
    assert_eq!(queued.conflict_strategy, ConflictStrategy::Overwrite);
    assert_eq!(queued.filename.as_deref(), Some("watchlist.json"));
    assert_eq!(queued.size, TMDB_EXPORT.len() as i64);
    assert_eq!(queued.counts.total, 0);

    // Before the worker runs: the import, and no rows yet.
    let before: ImportDetail = client
        .get(&format!("/api/v1/imports/{}", queued.id))
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(before.import, queued);
    assert!(before.items.is_empty());

    // The upload queued a job; the worker imports it.
    assert!(work(&env).await >= 1);
    let after: ImportDetail = client
        .get(&format!("/api/v1/imports/{}", queued.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(after.import.status, ImportStatus::Completed);
    assert!(after.import.started_at.is_some() && after.import.completed_at.is_some());
    assert_eq!(after.import.counts.total, 2);
    assert_eq!(after.import.counts.applied, 1);
    assert_eq!(after.import.counts.unmatched, 1);
    let rows: Vec<i32> = after.items.iter().map(|i| i.row).collect();
    assert_eq!(rows, [1, 2]);
    let matched = &after.items[0];
    assert_eq!(matched.outcome, ImportOutcome::Applied);
    let title = matched.title.as_ref().expect("a matched title");
    assert_eq!(title.reference.to_string(), FIGHT_CLUB);
    // The summary carries the caller's library state, which the import set.
    let state = title.library.as_ref().expect("in the library now");
    assert_eq!(state.status, Status::Completed);
    assert_eq!(state.score, Some(90));
    assert_eq!(matched.confidence, Some(1.0));
    assert_eq!(after.items[1].outcome, ImportOutcome::Unmatched);
    assert!(after.items[1].title.is_none());
    // And the library holds it.
    let held = LibraryService::new(env.db.db.clone())
        .all(ada.id())
        .await
        .unwrap();
    assert_eq!(held.len(), 1);

    // Listing, newest first, paged.
    let second: Import = upload(
        &client,
        &ada,
        &[("source", "mal_json")],
        Some(("mal.json", b"[]")),
    )
    .send()
    .await
    .assert_status(StatusCode::ACCEPTED)
    .json();
    assert_eq!(second.conflict_strategy, ConflictStrategy::Skip);
    let first: Page<Import> = client
        .get("/api/v1/imports?limit=1")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(first.items[0].id, second.id);
    let next = first.next_cursor.expect("a second page");
    let rest: Page<Import> = client
        .get(&format!("/api/v1/imports?limit=1&cursor={next}"))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(rest.items[0].id, queued.id);
    assert_eq!(rest.next_cursor, None);
    client
        .get("/api/v1/imports?cursor=AAAA")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));
    let theirs: Page<Import> = client
        .get("/api/v1/imports")
        .session(&eve)
        .send()
        .await
        .json();
    assert!(theirs.items.is_empty());

    // Another account's import is nobody's.
    for id in [queued.id, Uuid::now_v7()] {
        client
            .get(&format!("/api/v1/imports/{id}"))
            .session(&eve)
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND)
            .assert_problem_type(&support::problem("import-not-found"));
    }

    // Resolving the unmatched row by hand: a title that does not exist is
    // refused, a real one is applied and answered with the row.
    client
        .patch(&format!("/api/v1/imports/{}/items/2", queued.id))
        .session(&ada)
        .json(&json!({ "action": "match", "title": "tmdb:movie:999999" }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("title-not-found"));
    let resolved: ResolvedImportItem = client
        .patch(&format!("/api/v1/imports/{}/items/2", queued.id))
        .session(&ada)
        .json(&json!({ "action": "match", "title": BREAKING_BAD }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(resolved.previous_entry, None, "it had no title before");
    let resolved = resolved.item;
    assert_eq!(resolved.row, 2);
    assert_eq!(resolved.outcome, ImportOutcome::Applied);
    assert_eq!(resolved.confidence, Some(1.0));
    let source = resolved.source.expect("the row as the file has it");
    assert_eq!(source.title.as_deref(), Some("Lost"));
    assert_eq!(source.source_id.as_deref(), Some("tmdb:999999"));
    let title = resolved.title.expect("the title it now is");
    assert_eq!(title.reference.to_string(), BREAKING_BAD);
    assert_eq!(
        title.library.expect("in the library").status,
        Status::Planning
    );
    let after: ImportDetail = client
        .get(&format!("/api/v1/imports/{}", queued.id))
        .session(&ada)
        .send()
        .await
        .json();
    assert_eq!(
        (after.import.counts.applied, after.import.counts.unmatched),
        (2, 0)
    );
    // A row matched to the wrong title is re-pointed: the entry it added
    // goes to the trash, and the row applies to the right title instead.
    let fight_club = held[0].media.id;
    let moved: ResolvedImportItem = client
        .patch(&format!("/api/v1/imports/{}/items/1", queued.id))
        .session(&ada)
        .json(&json!({ "action": "match", "title": REPLY_1988 }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(moved.previous_entry, Some(PreviousEntry::Trashed));
    let moved = moved.item;
    assert_eq!(
        (moved.outcome, moved.confidence),
        (ImportOutcome::Applied, Some(1.0))
    );
    let title = moved.title.expect("the title it now is");
    assert_eq!(title.reference.to_string(), REPLY_1988);
    assert_eq!(title.library.expect("in the library").score, Some(90));
    let library = LibraryService::new(env.db.db.clone());
    let trash = library.trash(ada.id()).await.unwrap();
    assert_eq!(
        trash.iter().map(|e| e.media.id).collect::<Vec<_>>(),
        [fight_club]
    );
    assert!(
        library
            .plays(ada.id(), fight_club)
            .await
            .unwrap()
            .iter()
            .all(|p| p.voided_at.is_some())
    );
    // Re-pointed at the title it has, the row is answered as it is.
    let same: ImportItem = client
        .patch(&format!("/api/v1/imports/{}/items/1", queued.id))
        .session(&ada)
        .json(&json!({ "action": "match", "title": REPLY_1988 }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(same.outcome, ImportOutcome::Applied);
    assert_eq!(library.trash(ada.id()).await.unwrap().len(), 1);
    // A row with a title is not skipped, and a resolution is exactly one
    // action: no title on a skip, a title on a match, and no bare title.
    client
        .patch(&format!("/api/v1/imports/{}/items/1", queued.id))
        .session(&ada)
        .json(&json!({ "action": "skip" }))
        .send()
        .await
        .assert_status(StatusCode::CONFLICT)
        .assert_problem_type(&support::problem("import-item-not-resolvable"));
    for body in [
        json!({ "action": "skip", "title": FIGHT_CLUB }),
        json!({ "action": "match" }),
        json!({ "title": FIGHT_CLUB }),
        json!({ "action": "forget" }),
    ] {
        client
            .patch(&format!("/api/v1/imports/{}/items/1", queued.id))
            .session(&ada)
            .json(&body)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }
    client
        .patch(&format!("/api/v1/imports/{}/items/9", queued.id))
        .session(&ada)
        .json(&json!({ "action": "match", "title": FIGHT_CLUB }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("import-item-not-found"));
    client
        .patch(&format!("/api/v1/imports/{}/items/1", queued.id))
        .session(&eve)
        .json(&json!({ "action": "match", "title": FIGHT_CLUB }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("import-not-found"));
    client
        .patch(&format!("/api/v1/imports/{}/items/0", queued.id))
        .session(&ada)
        .json(&json!({ "action": "match", "title": FIGHT_CLUB }))
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    client.assert_conformance();
}

#[tokio::test]
async fn the_upload_refuses_what_it_cannot_import() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    env.metadata.set_missing("tmdb:movie:999999");

    // A file its source's parser refuses is never queued.
    for (source, file) in [
        ("mal_xml", TMDB_EXPORT.as_bytes()),
        ("tmdb_json", b"<myanimelist/>".as_slice()),
        ("mal_xml", b"\x1f\x8b\x08\x00".as_slice()),
    ] {
        client
            .post("/api/v1/imports")
            .bearer(&ada.write)
            .body(&multipart(), form(&[("source", source)], Some(("f", file))))
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem("unreadable-import"));
    }
    let none: Page<Import> = client
        .get("/api/v1/imports")
        .session(&ada)
        .send()
        .await
        .json();
    assert!(none.items.is_empty());
    assert_eq!(work(&env).await, 0, "nothing was queued");

    // A source or strategy no importer has is the body's 422.
    for fields in [
        [("source", "letterboxd_csv"), ("conflict_strategy", "skip")],
        [("source", "tmdb_json"), ("conflict_strategy", "replace")],
    ] {
        upload(&client, &ada, &fields, Some(("f", b"[]")))
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
    }
    // Malformed multipart, and multipart with no boundary, are 400s.
    client
        .post("/api/v1/imports")
        .session(&ada)
        .body(&multipart(), "not multipart at all")
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    client
        .post("/api/v1/imports")
        .session(&ada)
        .body(
            "multipart/form-data",
            form(&[("source", "tmdb_json")], None),
        )
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    // Not multipart at all.
    client
        .post("/api/v1/imports")
        .session(&ada)
        .json(&json!({ "source": "tmdb_json" }))
        .send()
        .await
        .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    // Past the deployment's cap, declared or streamed.
    upload(
        &client,
        &ada,
        &[("source", "tmdb_json")],
        Some(("f", b"[]")),
    )
    .content_length(support::settings().import_max_bytes + 1)
    .send()
    .await
    .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    let small = env.client_with(api::app::Settings {
        import_max_bytes: 512,
        ..support::settings()
    });
    let big = format!("[{}]", vec!["{}"; 400].join(","));
    upload(
        &small,
        &ada,
        &[("source", "tmdb_json")],
        Some(("f", big.as_bytes())),
    )
    .send()
    .await
    .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    // The cap is the import's alone: the default still applies elsewhere.
    small
        .get("/api/v1/imports")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK);

    client.assert_conformance();
    small.assert_conformance();

    // A database that refuses the import is a 500 that says nothing more.
    let degraded = env.degraded_client(support::db::Credential::Session).await;
    let failed = upload(
        &degraded,
        &ada,
        &[("source", "tmdb_json")],
        Some(("f", TMDB_EXPORT.as_bytes())),
    )
    .send()
    .await;
    failed
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR)
        .assert_problem_type(&support::problem("internal"));
    degraded.assert_conformance();

    // The statuses the generic sweep cannot reach, in this environment.
    let queued: Import = upload(
        &client,
        &ada,
        &[("source", "tmdb_json")],
        Some(("f", TMDB_EXPORT.as_bytes())),
    )
    .send()
    .await
    .assert_status(StatusCode::ACCEPTED)
    .json();
    client
        .get(&format!("/api/v1/imports/{}", queued.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK);
    work(&env).await;
    client
        .patch(&format!("/api/v1/imports/{}/items/1", queued.id))
        .session(&ada)
        .json(&json!({ "action": "skip" }))
        .send()
        .await
        .assert_status(StatusCode::CONFLICT);
    // Skipping the unmatched row sets it aside, and the counts move with it.
    let skipped: ImportItem = client
        .patch(&format!("/api/v1/imports/{}/items/2", queued.id))
        .session(&ada)
        .json(&json!({ "action": "skip" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(skipped.outcome, ImportOutcome::Skipped);
    assert!(skipped.title.is_none());
    let counts = client
        .get(&format!("/api/v1/imports/{}", queued.id))
        .session(&ada)
        .send()
        .await
        .json::<ImportDetail>()
        .import
        .counts;
    assert_eq!(
        (counts.applied, counts.skipped, counts.unmatched),
        (1, 1, 0)
    );
    // A skipped row can still be matched.
    client
        .patch(&format!("/api/v1/imports/{}/items/2", queued.id))
        .session(&ada)
        .json(&json!({ "action": "match", "title": FIGHT_CLUB }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    client.assert_conformance();

    // Restoring a backup: what it refuses, and a failing database.
    let restore = |client, query, file| restore_request(client, &ada, query, file);
    for (file, problem) in [
        (b"not json".as_slice(), "unreadable-backup"),
        (
            br#"{"format": "something-else"}"#.as_slice(),
            "unreadable-backup",
        ),
        (
            br#"{"format": "stacks-backup", "format_version": 99}"#.as_slice(),
            "unsupported-backup-version",
        ),
    ] {
        restore(&client, "", file)
            .send()
            .await
            .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
            .assert_problem_type(&support::problem(problem));
    }
    let mut dangling = empty_backup();
    dangling["blocklist"] = json!([{
        "id": Uuid::now_v7(),
        "title_id": Uuid::now_v7(),
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    }]);
    let dangling = dangling.to_string();
    restore(&client, "", dangling.as_bytes())
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .assert_problem_type(&support::problem("invalid-backup"));
    let empty = empty_backup().to_string();
    restore(&client, "", empty.as_bytes())
        .content_length(support::settings().import_max_bytes + 1)
        .send()
        .await
        .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    restore(&client, "?mode=replace", empty.as_bytes())
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    restore(&client, "?dry_run=true", empty.as_bytes())
        .send()
        .await
        .assert_status(StatusCode::OK);
    restore(&degraded, "", empty.as_bytes())
        .send()
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR)
        .assert_problem_type(&support::problem("internal"));
    client.assert_conformance();
    degraded.assert_conformance();

    support::sweep::sweep(&env, "imports").await;
    env.assert_covered("imports");
}

fn restore_request<'c>(
    client: &'c TestClient<App>,
    user: &TestUser,
    query: &str,
    file: &[u8],
) -> TestRequest<'c, App> {
    client
        .post(&format!("/api/v1/imports/backup{query}"))
        .session(user)
        .body(&multipart(), form(&[], Some(("backup.json", file))))
}

#[tokio::test]
async fn an_import_waits_out_a_metadata_outage_row_by_row() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    env.metadata
        .set_all_down(Some(support::metadata::unavailable()));
    let mal = br#"[{"mal_id": 21, "title": "One Piece", "status": "watching",
                    "episodes_watched": 3}]"#;
    let queued: Import = upload(
        &client,
        &ada,
        &[("source", "mal_json")],
        Some(("mal.json", mal)),
    )
    .send()
    .await
    .assert_status(StatusCode::ACCEPTED)
    .json();
    work(&env).await;
    let waiting: ImportDetail = client
        .get(&format!("/api/v1/imports/{}", queued.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(waiting.import.status, ImportStatus::Pending);
    assert_eq!(
        (
            waiting.import.counts.total,
            waiting.import.counts.pending_retry
        ),
        (1, 1)
    );
    assert_eq!(waiting.items[0].outcome, ImportOutcome::PendingRetry);
    assert!(waiting.items[0].error.is_some());
    let source = waiting.items[0].source.as_ref().unwrap();
    assert_eq!(source.source_id.as_deref(), Some("mal:21"));
    client.assert_conformance();
}

/// A valid backup holding nothing.
fn empty_backup() -> serde_json::Value {
    json!({
        "format": "stacks-backup",
        "format_version": 1,
        "exported_at": "2026-10-01T00:00:00Z",
        "generator": "test",
        "secrets_included": false,
        "user": null,
        "settings": {},
        "titles": [],
        "entries": [],
        "lists": [],
        "tags": [],
        "blocklist": [],
        "notification_channels": []
    })
}

#[tokio::test]
async fn a_backup_downloaded_from_one_account_restores_into_another() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let eve = env.user(Role::User).await;
    let media =
        stacks_core::media::MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()))
            .ensure_in_catalog(&support::metadata::id(FIGHT_CLUB))
            .await
            .unwrap();
    let library = LibraryService::new(env.db.db.clone());
    library
        .log_play(
            ada.id(),
            &media,
            stacks_core::library::LogPlay::manual(None),
        )
        .await
        .unwrap();

    let file = client
        .get("/api/v1/exports/backup")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .bytes()
        .clone();
    let upload = |query: &str| {
        client
            .post(&format!("/api/v1/imports/backup{query}"))
            .bearer(&eve.write)
            .body(&multipart(), form(&[], Some(("backup.json", &file))))
    };
    let dry: RestoreReport = upload("?dry_run=true")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(dry.dry_run);
    assert_eq!(dry.mode, RestoreMode::Merge);
    assert_eq!(dry.entries.inserted, 1);
    assert!(
        library.all(eve.id()).await.unwrap().is_empty(),
        "a dry run keeps nothing"
    );

    let real: RestoreReport = upload("?mode=overwrite")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(!real.dry_run);
    assert_eq!(real.mode, RestoreMode::Overwrite);
    assert_eq!((real.entries.inserted, real.plays.inserted), (1, 1));
    assert_eq!(real.changes[0].title, FIGHT_CLUB);
    assert_eq!(real.changes[0].action, RestoreAction::Inserted);
    let held = library.all(eve.id()).await.unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(
        held[0].entry.status,
        stacks_core::entity::enums::EntryStatus::Completed
    );
    assert_eq!(held[0].progress.as_ref().unwrap().play_count, 1);

    // Again: everything is already there.
    let again: RestoreReport = upload("").send().await.assert_status(StatusCode::OK).json();
    assert_eq!(again.entries.unchanged, 1);
    assert_eq!(again.changes[0].action, RestoreAction::Unchanged);
    client.assert_conformance();
}
