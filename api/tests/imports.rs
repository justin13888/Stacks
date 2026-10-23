//! Uploading an export, following its import, and the upload's refusals.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use api::app::App;
use api::catalog::Status;
use api::imports::{ConflictStrategy, Import, ImportDetail, ImportOutcome, ImportStatus};
use api::page::Page;
use kynos::http::StatusCode;
use kynos::test::{TestClient, TestRequest};
use serde_json::json;
use stacks_core::entity::enums::Role;
use stacks_core::jobs::{JobContext, JobRunner};
use stacks_core::library::LibraryService;
use support::metadata::FIGHT_CLUB;
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

    // Resolving a row by hand is issue #7's: a 501, after the 404s.
    let not_yet = client
        .patch(&format!("/api/v1/imports/{}/items/2", queued.id))
        .session(&ada)
        .json(&json!({ "title": FIGHT_CLUB }))
        .send()
        .await;
    not_yet
        .assert_status(StatusCode::NOT_IMPLEMENTED)
        .assert_problem_type(&support::problem("not-implemented"));
    assert!(not_yet.text().contains("issues/7"));
    client
        .patch(&format!("/api/v1/imports/{}/items/9", queued.id))
        .session(&ada)
        .json(&json!({ "title": FIGHT_CLUB }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("import-item-not-found"));
    client
        .patch(&format!("/api/v1/imports/{}/items/1", queued.id))
        .session(&eve)
        .json(&json!({ "title": FIGHT_CLUB }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("import-not-found"));
    client
        .patch(&format!("/api/v1/imports/{}/items/0", queued.id))
        .session(&ada)
        .json(&json!({ "title": FIGHT_CLUB }))
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
        .json(&json!({ "title": FIGHT_CLUB }))
        .send()
        .await
        .assert_status(StatusCode::NOT_IMPLEMENTED);
    client.assert_conformance();

    support::sweep::sweep(&env, "imports").await;
    env.assert_covered("imports");
}
