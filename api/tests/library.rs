//! The caller's library: entries, runs, plays, the event log, up next and
//! the calendar.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use api::app::App;
use api::catalog::Status;
use api::library::{
    CalendarEpisode, Change, EntryEvent, HistoryEvent, LibraryEntry, LibraryEntryDetail,
    LoggedPlays, Play, PlaySource, Precision, UpNextItem, WatchRun,
};
use api::page::Page;
use chrono::{Duration, NaiveDate, Utc};
use kynos::http::StatusCode;
use kynos::test::TestClient;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use stacks_core::entity::enums::Role;
use stacks_core::media::{MediaService, catalog};
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, ONE_PIECE, REPLY_1988};
use support::{Credentials, Env, TestUser};

type Client = TestClient<App>;

fn entry_url(reference: &str) -> String {
    format!("/api/v1/library/{reference}")
}

async fn get<T: DeserializeOwned>(client: &Client, user: &TestUser, url: &str) -> T {
    client
        .get(url)
        .session(user)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
}

async fn put(
    client: &Client,
    user: &TestUser,
    reference: &str,
    body: Value,
    expect: StatusCode,
) -> LibraryEntry {
    client
        .put(&entry_url(reference))
        .session(user)
        .json(&body)
        .send()
        .await
        .assert_status(expect)
        .json()
}

async fn patch(client: &Client, user: &TestUser, reference: &str, body: Value) -> LibraryEntry {
    client
        .patch(&entry_url(reference))
        .session(user)
        .json(&body)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
}

async fn plays(client: &Client, user: &TestUser, reference: &str, body: Value) -> Vec<Play> {
    let logged: LoggedPlays = client
        .post(&format!("{}/plays", entry_url(reference)))
        .session(user)
        .json(&body)
        .send()
        .await
        .assert_status(StatusCode::CREATED)
        .json();
    logged.plays
}

async fn refused(
    client: &Client,
    user: &TestUser,
    method: &str,
    url: &str,
    body: Option<Value>,
    status: StatusCode,
    slug: &str,
) {
    let mut request = match method {
        "get" => client.get(url),
        "put" => client.put(url),
        "patch" => client.patch(url),
        "post" => client.post(url),
        _ => client.delete(url),
    }
    .session(user);
    if let Some(body) = body {
        request = request.json(&body);
    }
    request
        .send()
        .await
        .assert_status(status)
        .assert_problem_type(&support::problem(slug));
}

/// Every page of a collection, through its cursors.
async fn all_pages<T: DeserializeOwned>(
    client: &Client,
    user: &TestUser,
    url: &str,
    limit: u32,
) -> Vec<T> {
    let sep = if url.contains('?') { '&' } else { '?' };
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut target = format!("{url}{sep}limit={limit}");
        if let Some(c) = &cursor {
            target.push_str(&format!("&cursor={c}"));
        }
        let page: Page<T> = get(client, user, &target).await;
        assert!(page.items.len() <= limit as usize);
        out.extend(page.items);
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    out
}

fn refs(entries: &[LibraryEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| e.title.reference.to_string())
        .collect()
}

// ---------------------------------------------------------------- entries

async fn entries_replace_merge_trash_and_restore(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;

    // Create: 201, with where it lives.
    let created = client
        .put(&entry_url(FIGHT_CLUB))
        .session(&ada)
        .json(&json!({ "status": "planning", "score": 80, "favourite": true, "notes": "n" }))
        .send()
        .await;
    created.assert_status(StatusCode::CREATED);
    let location = created
        .header("location")
        .expect("a location")
        .replace("%3A", ":");
    assert_eq!(location, entry_url(FIGHT_CLUB));
    let entry: LibraryEntry = created.json();
    assert_eq!(entry.title.reference.to_string(), FIGHT_CLUB);
    assert_eq!(entry.title.title, "Fight Club");
    assert_eq!(
        (
            entry.status,
            entry.score,
            entry.favourite,
            entry.notes.as_deref()
        ),
        (Status::Planning, Some(80), true, Some("n"))
    );
    assert_eq!(
        entry.title.library.as_ref().map(|l| l.status),
        Some(Status::Planning)
    );
    assert_eq!(entry.deleted_at, None);

    // Replace: 200, and every member left out is cleared.
    let replaced = put(
        &client,
        &ada,
        FIGHT_CLUB,
        json!({ "status": "completed" }),
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        (
            replaced.status,
            replaced.score,
            replaced.favourite,
            replaced.notes
        ),
        (Status::Completed, None, false, None)
    );
    assert_eq!(replaced.added_at, entry.added_at);

    // Merge: members left out are never touched.
    let merged = patch(
        &client,
        &ada,
        FIGHT_CLUB,
        json!({ "notes": "keep", "favourite": true }),
    )
    .await;
    assert_eq!(merged.status, Status::Completed);
    let scored = patch(&client, &ada, FIGHT_CLUB, json!({ "score": 90 })).await;
    assert_eq!(
        (
            scored.status,
            scored.score,
            scored.favourite,
            scored.notes.as_deref()
        ),
        (Status::Completed, Some(90), true, Some("keep"))
    );
    let cleared = patch(&client, &ada, FIGHT_CLUB, json!({ "notes": null })).await;
    assert_eq!(
        (cleared.score, cleared.notes, cleared.favourite),
        (Some(90), None, true)
    );
    let same = patch(&client, &ada, FIGHT_CLUB, json!({})).await;
    assert_eq!(same.updated_at, cleared.updated_at);
    // Out of range is the schema's 422.
    client
        .patch(&entry_url(FIGHT_CLUB))
        .session(&ada)
        .json(&json!({ "score": 101 }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    // Detail: the completion opened run 1, completed today.
    let detail: LibraryEntryDetail = get(&client, &ada, &entry_url(FIGHT_CLUB)).await;
    assert_eq!(detail.entry, same);
    assert_eq!(detail.runs.len(), 1);
    assert_eq!(detail.runs[0].number, 1);
    assert_eq!(
        detail.runs[0].completed.map(|d| d.date),
        Some(Utc::now().date_naive())
    );
    assert!(detail.watched.is_empty());

    // Trash, and back exactly as it was.
    client
        .delete(&entry_url(FIGHT_CLUB))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    for (method, body) in [("get", None), ("patch", Some(json!({}))), ("delete", None)] {
        refused(
            &client,
            &ada,
            method,
            &entry_url(FIGHT_CLUB),
            body,
            StatusCode::NOT_FOUND,
            "entry-not-found",
        )
        .await;
    }
    let trash: Page<LibraryEntry> = get(&client, &ada, "/api/v1/library?deleted=true").await;
    assert_eq!(refs(&trash.items), [FIGHT_CLUB]);
    assert!(trash.items[0].deleted_at.is_some());
    let live: Page<LibraryEntry> = get(&client, &ada, "/api/v1/library").await;
    assert!(live.items.is_empty());

    let restored: LibraryEntry = client
        .post(&format!("{}/restorations", entry_url(FIGHT_CLUB)))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        (
            restored.status,
            restored.score,
            restored.favourite,
            restored.notes.clone(),
            restored.added_at,
            restored.progress.clone(),
            restored.deleted_at,
        ),
        (
            same.status,
            same.score,
            same.favourite,
            same.notes.clone(),
            same.added_at,
            same.progress.clone(),
            None,
        )
    );
    let back: LibraryEntryDetail = get(&client, &ada, &entry_url(FIGHT_CLUB)).await;
    assert_eq!(back.runs, detail.runs);
    refused(
        &client,
        &ada,
        "post",
        &format!("{}/restorations", entry_url(FIGHT_CLUB)),
        None,
        StatusCode::NOT_FOUND,
        "trashed-entry-not-found",
    )
    .await;
    refused(
        &client,
        &ada,
        "post",
        &format!("{}/restorations", entry_url("tmdb:movie:424242")),
        None,
        StatusCode::NOT_FOUND,
        "trashed-entry-not-found",
    )
    .await;

    // A PUT of a trashed title brings the same entry back, replaced.
    client
        .delete(&entry_url(FIGHT_CLUB))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let again = put(
        &client,
        &ada,
        FIGHT_CLUB,
        json!({ "status": "paused", "score": null }),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(again.added_at, entry.added_at);
    assert_eq!((again.status, again.score), (Status::Paused, None));

    // A title the provider does not have is a 404; an outage is not.
    env.metadata.set_missing("tmdb:movie:999999");
    refused(
        &client,
        &ada,
        "put",
        &entry_url("tmdb:movie:999999"),
        Some(json!({ "status": "planning" })),
        StatusCode::NOT_FOUND,
        "title-not-found",
    )
    .await;
    env.metadata.set_all_down(Some(metadata::unavailable()));
    let placeholder = put(
        &client,
        &ada,
        "tmdb:movie:777",
        json!({ "status": "planning" }),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(placeholder.title.title, "tmdb:movie:777");
    env.metadata.set_all_down(None);

    // Someone else's library is not this one.
    let bob = env.user(Role::User).await;
    refused(
        &client,
        &bob,
        "get",
        &entry_url(FIGHT_CLUB),
        None,
        StatusCode::NOT_FOUND,
        "entry-not-found",
    )
    .await;
    // Tokens: `read` reads, `write` writes.
    client
        .get(&entry_url(FIGHT_CLUB))
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .patch(&entry_url(FIGHT_CLUB))
        .bearer(&ada.write)
        .json(&json!({ "favourite": false }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .patch(&entry_url(FIGHT_CLUB))
        .bearer(&ada.read)
        .json(&json!({ "favourite": false }))
        .send()
        .await
        .assert_status(StatusCode::FORBIDDEN);

    client.assert_conformance();
}

#[tokio::test]
async fn entries_replace_merge_trash_and_restore_exactly() {
    let env = env!();
    entries_replace_merge_trash_and_restore(&env).await;
}

async fn listing_filters_sorts_and_pages(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let shelf = [
        (FIGHT_CLUB, json!({ "status": "completed", "score": 50 })),
        (
            BREAKING_BAD,
            json!({ "status": "watching", "score": 90, "favourite": true }),
        ),
        (REPLY_1988, json!({ "status": "planning" })),
        (ONE_PIECE, json!({ "status": "watching", "score": 70 })),
        ("tmdb:movie:2", json!({ "status": "dropped", "score": 70 })),
    ];
    for (reference, body) in &shelf {
        put(&client, &ada, reference, body.clone(), StatusCode::CREATED).await;
    }
    // A change moves an entry to the front of `updated`, not of `added`.
    patch(
        &client,
        &ada,
        FIGHT_CLUB,
        json!({ "notes": "rewatch soon" }),
    )
    .await;
    let ids = |rs: &[&str]| rs.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

    let updated: Page<LibraryEntry> = get(&client, &ada, "/api/v1/library").await;
    assert_eq!(
        refs(&updated.items),
        ids(&[
            FIGHT_CLUB,
            "tmdb:movie:2",
            ONE_PIECE,
            REPLY_1988,
            BREAKING_BAD
        ])
    );
    assert_eq!(updated.next_cursor, None);
    for (query, expected) in [
        (
            "",
            ids(&[
                FIGHT_CLUB,
                "tmdb:movie:2",
                ONE_PIECE,
                REPLY_1988,
                BREAKING_BAD,
            ]),
        ),
        (
            "?sort=added",
            ids(&[
                "tmdb:movie:2",
                ONE_PIECE,
                REPLY_1988,
                BREAKING_BAD,
                FIGHT_CLUB,
            ]),
        ),
        (
            "?sort=title",
            ids(&[
                BREAKING_BAD,
                FIGHT_CLUB,
                ONE_PIECE,
                REPLY_1988,
                "tmdb:movie:2",
            ]),
        ),
        (
            "?sort=score",
            ids(&[
                BREAKING_BAD,
                "tmdb:movie:2",
                ONE_PIECE,
                FIGHT_CLUB,
                REPLY_1988,
            ]),
        ),
        (
            "?status=watching&sort=title",
            ids(&[BREAKING_BAD, ONE_PIECE]),
        ),
        ("?category=anime", ids(&[ONE_PIECE])),
        ("?category=k-drama", ids(&[REPLY_1988])),
        ("?favourite=true", ids(&[BREAKING_BAD])),
        ("?q=REPLY", ids(&[REPLY_1988])),
        ("?q=%25", ids(&[])),
    ] {
        // Small pages walk the same order, with nothing repeated or missed.
        for limit in [1, 2, 50] {
            let walked: Vec<LibraryEntry> =
                all_pages(&client, &ada, &format!("/api/v1/library{query}"), limit).await;
            assert_eq!(refs(&walked), expected, "{query} in pages of {limit}");
        }
    }

    // A tag narrows to the entries carrying it.
    let tags = stacks_core::tags::TagService::new(env.db.db.clone());
    let tag = tags
        .create_tag(ada.id(), "comfort".into(), None)
        .await
        .unwrap();
    let row = catalog::find_by_ref(&env.db.db, &metadata::id(REPLY_1988))
        .await
        .unwrap()
        .unwrap();
    tags.assign_tag(ada.id(), row.id, tag.id).await.unwrap();
    let tagged: Page<LibraryEntry> =
        get(&client, &ada, &format!("/api/v1/library?tag={}", tag.id)).await;
    assert_eq!(refs(&tagged.items), [REPLY_1988]);

    // A cursor continues only the listing it came from.
    let first: Page<LibraryEntry> = get(&client, &ada, "/api/v1/library?sort=title&limit=2").await;
    let cursor = first.next_cursor.expect("more follow");
    for other in [
        format!("/api/v1/library?sort=score&limit=2&cursor={cursor}"),
        format!("/api/v1/library?limit=2&cursor={cursor}"),
        format!("/api/v1/library?sort=title&status=watching&limit=2&cursor={cursor}"),
        format!("/api/v1/library?sort=title&deleted=true&limit=2&cursor={cursor}"),
        "/api/v1/library?cursor=bm90LWEtY3Vyc29y".to_owned(),
        "/api/v1/library?cursor=!!".to_owned(),
    ] {
        refused(
            &client,
            &ada,
            "get",
            &other,
            None,
            StatusCode::BAD_REQUEST,
            "invalid-cursor",
        )
        .await;
    }
    client
        .get("/api/v1/library?sort=random")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    client.assert_conformance();
}

#[tokio::test]
async fn the_library_lists_filters_sorts_and_pages() {
    let env = env!();
    listing_filters_sorts_and_pages(&env).await;
}

// ------------------------------------------------------- plays and runs

async fn plays_runs_and_events(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;

    // A movie play tracks and completes the title.
    let movie = plays(&client, &ada, FIGHT_CLUB, json!({ "kind": "movie" })).await;
    assert_eq!(movie.len(), 1);
    assert_eq!(
        (movie[0].run, movie[0].season, movie[0].episode),
        (1, None, None)
    );
    assert_eq!(movie[0].source, PlaySource::Manual);
    assert!(movie[0].watched_at.is_some());
    let fc: LibraryEntryDetail = get(&client, &ada, &entry_url(FIGHT_CLUB)).await;
    assert_eq!(fc.entry.status, Status::Completed);
    assert_eq!(fc.entry.progress.plays, 1);

    // The kind has to fit the title.
    let plays_url = |r: &str| format!("{}/plays", entry_url(r));
    for (reference, body) in [
        (
            FIGHT_CLUB,
            json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": 1 }] }),
        ),
        (
            FIGHT_CLUB,
            json!({ "kind": "through", "season": 1, "episode": 1 }),
        ),
        (BREAKING_BAD, json!({ "kind": "movie" })),
    ] {
        refused(
            &client,
            &ada,
            "post",
            &plays_url(reference),
            Some(body),
            StatusCode::UNPROCESSABLE_ENTITY,
            "wrong-play-kind",
        )
        .await;
    }

    // Episodes: a play each, and the show is tracked as watching.
    let watched_at = "2024-03-01T20:00:00Z";
    let first = plays(
        &client,
        &ada,
        BREAKING_BAD,
        json!({
            "kind": "episodes",
            "episodes": [{ "season": 1, "episode": 1 }, { "season": 1, "episode": 2 }],
            "watched_at": watched_at,
        }),
    )
    .await;
    assert_eq!(
        first
            .iter()
            .map(|p| (p.season, p.episode))
            .collect::<Vec<_>>(),
        [(Some(1), Some(1)), (Some(1), Some(2))]
    );
    assert!(
        first.iter().all(|p| p.watched_at.map(|t| t.to_rfc3339())
            == Some("2024-03-01T20:00:00+00:00".to_owned()))
    );
    let bb: LibraryEntryDetail = get(&client, &ada, &entry_url(BREAKING_BAD)).await;
    assert_eq!(bb.entry.status, Status::Watching);
    for missing in [
        json!({ "kind": "episodes", "episodes": [{ "season": 9, "episode": 1 }] }),
        json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": 1 }, { "season": 1, "episode": 9 }] }),
        json!({ "kind": "through", "season": 1, "episode": 9 }),
        json!({ "kind": "through", "season": 3, "episode": 1 }),
    ] {
        refused(
            &client,
            &ada,
            "post",
            &plays_url(BREAKING_BAD),
            Some(missing),
            StatusCode::NOT_FOUND,
            "episode-not-found",
        )
        .await;
    }
    // A far-off season is refused after one lookup, not one per season
    // below it.
    let calls = env.metadata.season_calls();
    refused(
        &client,
        &ada,
        "post",
        &plays_url(BREAKING_BAD),
        Some(json!({ "kind": "through", "season": 10000, "episode": 1 })),
        StatusCode::NOT_FOUND,
        "episode-not-found",
    )
    .await;
    assert!(env.metadata.season_calls() - calls <= 1);
    env.metadata.set_missing("tmdb:show:404404");
    refused(
        &client,
        &ada,
        "post",
        &plays_url("tmdb:show:404404"),
        Some(json!({ "kind": "through", "season": 1, "episode": 1 })),
        StatusCode::NOT_FOUND,
        "title-not-found",
    )
    .await;
    // A refused batch logged nothing.
    let unchanged: LibraryEntryDetail = get(&client, &ada, &entry_url(BREAKING_BAD)).await;
    assert_eq!(unchanged.entry.progress.plays, 2);

    // Through: every episode not yet watched, up to and including S2E1.
    let through = plays(
        &client,
        &ada,
        BREAKING_BAD,
        json!({ "kind": "through", "season": 2, "episode": 1 }),
    )
    .await;
    assert_eq!(
        through
            .iter()
            .map(|p| (p.season, p.episode))
            .collect::<Vec<_>>(),
        [(Some(1), Some(3)), (Some(2), Some(1))]
    );
    assert!(
        through
            .iter()
            .all(|p| p.source == PlaySource::Bulk && p.watched_at.is_none())
    );
    let again = plays(
        &client,
        &ada,
        BREAKING_BAD,
        json!({ "kind": "through", "season": 2, "episode": 1 }),
    )
    .await;
    assert!(again.is_empty());
    let bb: LibraryEntryDetail = get(&client, &ada, &entry_url(BREAKING_BAD)).await;
    assert_eq!(
        bb.watched
            .iter()
            .map(|w| (w.season, w.episode))
            .collect::<Vec<_>>(),
        [(1, 1), (1, 2), (1, 3), (2, 1)]
    );
    assert_eq!(bb.entry.progress.current_run_episodes, 4);

    // Voiding keeps the play; it only stops counting.
    let voided = &through[1];
    client
        .delete(&format!("/api/v1/plays/{}", voided.id))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    refused(
        &client,
        &ada,
        "delete",
        &format!("/api/v1/plays/{}", voided.id),
        None,
        StatusCode::CONFLICT,
        "play-already-voided",
    )
    .await;
    let bob = env.user(Role::User).await;
    for (who, id) in [(&bob, first[0].id), (&ada, uuid::Uuid::now_v7())] {
        refused(
            &client,
            who,
            "delete",
            &format!("/api/v1/plays/{id}"),
            None,
            StatusCode::NOT_FOUND,
            "play-not-found",
        )
        .await;
    }
    let bb: LibraryEntryDetail = get(&client, &ada, &entry_url(BREAKING_BAD)).await;
    assert_eq!(bb.watched.len(), 3);
    assert_eq!(bb.entry.progress.plays, 3);
    let events: Page<EntryEvent> = get(
        &client,
        &ada,
        &format!("{}/events", entry_url(BREAKING_BAD)),
    )
    .await;
    match &events.items[0].change {
        Change::PlayVoided { play } => {
            assert_eq!(play.id, voided.id);
            assert_eq!((play.season, play.episode), (Some(2), Some(1)));
            assert!(play.voided_at.is_some());
        }
        other => panic!("expected the void first, got {other:?}"),
    }

    // A rewatch: run 2, back to watching, nothing watched in it yet.
    let runs_url = format!("{}/runs", entry_url(BREAKING_BAD));
    let started = client
        .post(&runs_url)
        .session(&ada)
        .json(&json!({ "started": { "date": "2024-05-01", "precision": "month" } }))
        .send()
        .await;
    started.assert_status(StatusCode::CREATED);
    assert_eq!(
        started.header("location").map(|l| l.replace("%3A", ":")),
        Some(format!("{runs_url}/2"))
    );
    let run: WatchRun = started.json();
    assert_eq!(run.number, 2);
    assert_eq!(run.started.map(|d| d.precision), Some(Precision::Month));
    assert_eq!(run.completed, None);
    let bb: LibraryEntryDetail = get(&client, &ada, &entry_url(BREAKING_BAD)).await;
    assert_eq!(bb.runs.len(), 2);
    assert!(bb.watched.is_empty());
    assert_eq!(bb.entry.progress.rewatches, 1);
    assert!(
        bb.entry
            .title
            .library
            .as_ref()
            .is_some_and(|l| l.rewatching)
    );

    // Run dates merge.
    let completed: WatchRun = client
        .patch(&format!("{runs_url}/2"))
        .session(&ada)
        .json(&json!({ "completed": { "date": "2024-06-02", "precision": "day" } }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(completed.started, run.started);
    assert_eq!(
        completed.completed.map(|d| d.date),
        NaiveDate::from_ymd_opt(2024, 6, 2)
    );
    let unstarted: WatchRun = client
        .patch(&format!("{runs_url}/2"))
        .session(&ada)
        .json(&json!({ "started": null }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(
        (unstarted.started, unstarted.completed),
        (None, completed.completed)
    );
    refused(
        &client,
        &ada,
        "patch",
        &format!("{runs_url}/9"),
        Some(json!({})),
        StatusCode::NOT_FOUND,
        "watch-run-not-found",
    )
    .await;
    for (method, url, body) in [
        (
            "patch",
            format!("{}/runs/1", entry_url(REPLY_1988)),
            json!({}),
        ),
        ("post", format!("{}/runs", entry_url(REPLY_1988)), json!({})),
    ] {
        refused(
            &client,
            &ada,
            method,
            &url,
            Some(body),
            StatusCode::NOT_FOUND,
            "entry-not-found",
        )
        .await;
    }

    // The event log: newest first, typed, paged without repeats.
    let log: Vec<EntryEvent> = all_pages(
        &client,
        &ada,
        &format!("{}/events", entry_url(BREAKING_BAD)),
        50,
    )
    .await;
    let paged: Vec<EntryEvent> = all_pages(
        &client,
        &ada,
        &format!("{}/events", entry_url(BREAKING_BAD)),
        3,
    )
    .await;
    assert_eq!(log, paged);
    let mut ids: Vec<_> = log.iter().map(|e| e.id).collect();
    assert!(ids.windows(2).all(|w| w[0] > w[1]), "newest first");
    ids.dedup();
    assert_eq!(ids.len(), log.len());
    assert!(matches!(log[0].change, Change::RunUpdated { .. }));
    assert!(matches!(
        log.last().map(|e| &e.change),
        Some(Change::EntryCreated { .. })
    ));
    let logged = log
        .iter()
        .filter(|e| matches!(e.change, Change::PlayLogged { .. }))
        .count();
    assert_eq!(logged, 4);

    // A trashed entry keeps its log; a title never added has none.
    client
        .delete(&entry_url(BREAKING_BAD))
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::NO_CONTENT);
    let after: Page<EntryEvent> = get(
        &client,
        &ada,
        &format!("{}/events", entry_url(BREAKING_BAD)),
    )
    .await;
    assert!(matches!(after.items[0].change, Change::EntryDeleted));
    for never in [REPLY_1988, "tmdb:movie:31337"] {
        refused(
            &client,
            &ada,
            "get",
            &format!("{}/events", entry_url(never)),
            None,
            StatusCode::NOT_FOUND,
            "entry-not-found",
        )
        .await;
    }
    refused(
        &client,
        &ada,
        "get",
        &format!("{}/events?cursor=AA", entry_url(FIGHT_CLUB)),
        None,
        StatusCode::BAD_REQUEST,
        "invalid-cursor",
    )
    .await;

    // History: every title's events, newest first, with the title.
    let history: Vec<HistoryEvent> = all_pages(&client, &ada, "/api/v1/history", 2).await;
    let whole: Vec<HistoryEvent> = all_pages(&client, &ada, "/api/v1/history", 200).await;
    assert_eq!(history, whole);
    assert!(history.windows(2).all(|w| w[0].event.id > w[1].event.id));
    assert_eq!(history[0].title.reference.to_string(), BREAKING_BAD);
    assert!(matches!(history[0].event.change, Change::EntryDeleted));
    assert_eq!(
        history
            .last()
            .map(|h| h.title.reference.to_string())
            .as_deref(),
        Some(FIGHT_CLUB)
    );
    let bobs: Page<HistoryEvent> = get(&client, &bob, "/api/v1/history").await;
    assert!(bobs.items.is_empty());
    refused(
        &client,
        &ada,
        "get",
        "/api/v1/history?cursor=AA",
        None,
        StatusCode::BAD_REQUEST,
        "invalid-cursor",
    )
    .await;

    client.assert_conformance();
}

#[tokio::test]
async fn plays_are_logged_in_batches_voided_and_logged_as_events() {
    let env = env!();
    plays_runs_and_events(&env).await;
}

// ------------------------------------------------- up next and calendar

async fn up_next_and_calendar(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;
    let today = Utc::now().date_naive();
    // A show of this scenario's own, so what other scenarios stored in the
    // shared catalog cannot change what it sees.
    let show = "tmdb:show:6100";

    // The show through S1E3; Reply 1988 watching, nothing watched yet.
    plays(
        &client,
        &ada,
        show,
        json!({ "kind": "through", "season": 1, "episode": 3, "watched_at": "2024-01-01T00:00:00Z" }),
    )
    .await;
    put(
        &client,
        &ada,
        REPLY_1988,
        json!({ "status": "watching" }),
        StatusCode::CREATED,
    )
    .await;

    // Season 2's episodes are not stored yet; its summary's count still says
    // S2E1 comes next. The show watched most recently comes first.
    let shape = |items: &[UpNextItem]| {
        items
            .iter()
            .map(|i| (i.title.reference.to_string(), i.season, i.episode))
            .collect::<Vec<_>>()
    };
    let next: Page<UpNextItem> = get(&client, &ada, "/api/v1/up-next").await;
    assert_eq!(
        shape(&next.items),
        [(show.to_owned(), 2, 1), (REPLY_1988.to_owned(), 1, 1)]
    );
    assert_eq!(
        (
            next.items[0].name.clone(),
            next.items[0].air_date,
            next.items[0].aired
        ),
        (None, None, false)
    );

    // Reading the seasons stores their episodes.
    for (reference, season) in [(show, 2), (REPLY_1988, 1)] {
        client
            .get(&format!("/api/v1/titles/{reference}/seasons/{season}"))
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::OK);
    }
    let next: Vec<UpNextItem> = all_pages(&client, &ada, "/api/v1/up-next", 1).await;
    assert_eq!(
        shape(&next),
        [(show.to_owned(), 2, 1), (REPLY_1988.to_owned(), 1, 1)]
    );
    assert_eq!(next[0].name.as_deref(), Some("Episode 1"));
    assert_eq!(next[0].air_date, Some(today + Duration::days(7)));
    assert!(!next[0].aired);
    assert!(next[0].last_watched_at.is_some());
    assert!(next[1].aired);
    assert_eq!(next[1].last_watched_at, None);
    refused(
        &client,
        &ada,
        "get",
        "/api/v1/up-next?cursor=AA",
        None,
        StatusCode::BAD_REQUEST,
        "invalid-cursor",
    )
    .await;

    // The calendar: stored episodes in range, by date then title.
    let calendar = |from: NaiveDate, to: NaiveDate| format!("/api/v1/calendar?from={from}&to={to}");
    let day = |d| NaiveDate::from_ymd_opt(2020, 1, d).unwrap();
    let cal_shape = |items: &[CalendarEpisode]| {
        items
            .iter()
            .map(|i| {
                (
                    i.air_date,
                    i.title.reference.to_string(),
                    i.season,
                    i.episode,
                    i.watched,
                )
            })
            .collect::<Vec<_>>()
    };
    let aired: Vec<CalendarEpisode> = all_pages(&client, &ada, &calendar(day(6), day(7)), 1).await;
    assert_eq!(
        cal_shape(&aired),
        [
            (day(6), REPLY_1988.to_owned(), 1, 1, false),
            (day(6), show.to_owned(), 1, 1, true),
            (day(7), REPLY_1988.to_owned(), 1, 2, false),
            (day(7), show.to_owned(), 1, 2, true),
        ]
    );
    assert_eq!(aired[0].name.as_deref(), Some("Episode 1"));
    assert_eq!(aired[0].runtime_minutes, Some(47));
    let upcoming: Page<CalendarEpisode> =
        get(&client, &ada, &calendar(today, today + Duration::days(62))).await;
    assert_eq!(
        cal_shape(&upcoming.items),
        (1..=3)
            .map(|n| (
                today + Duration::days(6 + n),
                show.to_owned(),
                2,
                n as u32,
                false
            ))
            .collect::<Vec<_>>()
    );

    // A dropped show leaves the calendar and up next.
    patch(&client, &ada, REPLY_1988, json!({ "status": "dropped" })).await;
    let aired: Page<CalendarEpisode> = get(&client, &ada, &calendar(day(6), day(7))).await;
    assert_eq!(aired.items.len(), 2);
    assert!(
        aired
            .items
            .iter()
            .all(|i| i.title.reference.to_string() == show)
    );
    let next: Page<UpNextItem> = get(&client, &ada, "/api/v1/up-next").await;
    assert_eq!(shape(&next.items), [(show.to_owned(), 2, 1)]);

    // A range runs forwards and spans at most 62 days.
    for (from, to) in [(day(7), day(6)), (day(1), day(1) + Duration::days(63))] {
        refused(
            &client,
            &ada,
            "get",
            &calendar(from, to),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid-date-range",
        )
        .await;
    }
    refused(
        &client,
        &ada,
        "get",
        &format!("{}&cursor=AA", calendar(day(6), day(7))),
        None,
        StatusCode::BAD_REQUEST,
        "invalid-cursor",
    )
    .await;

    client.assert_conformance();
}

#[tokio::test]
async fn up_next_and_the_calendar_follow_the_runs() {
    let env = env!();
    up_next_and_calendar(&env).await;
}

// ---------------------------------------------------------------- coverage

#[tokio::test]
async fn library_covers_every_declared_response() {
    let env = env!();
    entries_replace_merge_trash_and_restore(&env).await;
    listing_filters_sorts_and_pages(&env).await;
    plays_runs_and_events(&env).await;
    up_next_and_calendar(&env).await;

    // An upstream failure blocks a play only when nothing is known of the
    // show's seasons.
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

    support::sweep::sweep(&env, "library").await;
    env.assert_covered("library");
}
