//! Dated changes: a movie play or a status change carries the day it
//! happened, which dates the runs it touches in the same request, and run
//! dates out of order are a 422 that changes nothing.

use super::support::metadata::{BREAKING_BAD, FIGHT_CLUB};
use super::support::{Credentials, Env};
use super::{entry_url, get, patch, plays, refused};
use api::catalog::Status;
use api::library::LibraryEntryDetail;
use kynos::http::StatusCode;
use serde_json::{Value, json};
use stacks_core::entity::enums::Role;

fn when(date: &str, precision: &str) -> Value {
    json!({ "date": date, "precision": precision })
}

/// Run `n`'s dates (1-based) as JSON, for comparing with [`when`].
fn run_dates(detail: &LibraryEntryDetail, n: usize) -> (Value, Value) {
    let run = &detail.runs[n - 1];
    (
        serde_json::to_value(run.started).unwrap(),
        serde_json::to_value(run.completed).unwrap(),
    )
}

pub(super) async fn dated_changes(env: &Env) {
    let client = env.client();
    let ada = env.user(Role::User).await;

    // A movie not in the library, watched in March 2019: one request
    // tracks it, completes it, and dates its first run then.
    let march = when("2019-03-01", "month");
    let logged = plays(
        &client,
        &ada,
        FIGHT_CLUB,
        json!({ "kind": "movie", "watched_at": "2019-03-01T12:00:00Z", "watched_on": march }),
    )
    .await;
    assert_eq!(logged.len(), 1);
    let movie: LibraryEntryDetail = get(&client, &ada, &entry_url(FIGHT_CLUB)).await;
    assert_eq!(movie.entry.status, Status::Completed);
    assert_eq!(movie.runs.len(), 1);
    assert_eq!(run_dates(&movie, 1), (march.clone(), march.clone()));

    // A rewatch cannot start before that run finished, and a run cannot
    // finish before it started; neither changes anything.
    let runs = format!("{}/runs", entry_url(FIGHT_CLUB));
    refused(
        &client,
        &ada,
        "post",
        &runs,
        Some(json!({ "started": when("2019-02-14", "day") })),
        StatusCode::UNPROCESSABLE_ENTITY,
        "run-dates-out-of-order",
    )
    .await;
    refused(
        &client,
        &ada,
        "patch",
        &format!("{runs}/1"),
        Some(json!({ "completed": when("2018-01-01", "year") })),
        StatusCode::UNPROCESSABLE_ENTITY,
        "run-dates-out-of-order",
    )
    .await;
    let unchanged: LibraryEntryDetail = get(&client, &ada, &entry_url(FIGHT_CLUB)).await;
    assert_eq!(unchanged.runs.len(), 1);
    assert_eq!(run_dates(&unchanged, 1), (march.clone(), march));

    // Completing a show on a date finishes its current run then, in the
    // same request; a date before the run started is refused.
    plays(
        &client,
        &ada,
        BREAKING_BAD,
        json!({ "kind": "episodes", "episodes": [{ "season": 1, "episode": 1 }] }),
    )
    .await;
    let started = when("2020-01-10", "day");
    client
        .patch(&format!("{}/runs/1", entry_url(BREAKING_BAD)))
        .session(&ada)
        .json(&json!({ "started": started }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    refused(
        &client,
        &ada,
        "patch",
        &entry_url(BREAKING_BAD),
        Some(json!({ "status": "completed", "on": when("2020-01-09", "day") })),
        StatusCode::UNPROCESSABLE_ENTITY,
        "run-dates-out-of-order",
    )
    .await;
    let watching: LibraryEntryDetail = get(&client, &ada, &entry_url(BREAKING_BAD)).await;
    assert_eq!(watching.entry.status, Status::Watching);
    assert_eq!(run_dates(&watching, 1), (started.clone(), Value::Null));

    let finished = when("2020-02-02", "day");
    let entry = patch(
        &client,
        &ada,
        BREAKING_BAD,
        json!({ "status": "completed", "on": finished }),
    )
    .await;
    assert_eq!(entry.status, Status::Completed);
    let show: LibraryEntryDetail = get(&client, &ada, &entry_url(BREAKING_BAD)).await;
    assert_eq!(run_dates(&show, 1), (started, finished));

    client.assert_conformance();
}
