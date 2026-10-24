//! Administering accounts and the job queue.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::HashSet;

use api::account::Role as ApiRole;
use api::admin::{AdminUser, Job, JobStatus};
use api::page::Page;
use kynos::http::StatusCode;
use serde_json::json;
use stacks_core::entity::enums::{JobType, Role};
use stacks_core::jobs::repo as jobs_repo;
use support::Credentials;
use uuid::Uuid;

#[tokio::test]
async fn administrators_page_accounts_and_change_roles() {
    let env = env!();
    let client = env.client();
    let root = env.user(Role::Admin).await;
    let ada = env.user(Role::User).await;
    let bob = env.user(Role::User).await;
    let admin_token = root.admin.clone().unwrap();

    // Pages cover every account once, newest first.
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let url = match &cursor {
            Some(c) => format!("/api/v1/admin/users?limit=2&cursor={c}"),
            None => "/api/v1/admin/users?limit=2".to_owned(),
        };
        let page: Page<AdminUser> = client
            .get(&url)
            .session(&root)
            .send()
            .await
            .assert_status(StatusCode::OK)
            .json();
        assert!(page.items.len() <= 2);
        seen.extend(page.items.iter().map(|u| u.id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(seen, [bob.id(), ada.id(), root.id()]);
    let listed: Page<AdminUser> = client
        .get("/api/v1/admin/users")
        .bearer(&admin_token)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    let shown = listed.items.iter().find(|u| u.id == ada.id()).unwrap();
    assert_eq!(shown.role, ApiRole::User);
    assert_eq!(shown.deactivated_at, None);
    client
        .get("/api/v1/admin/users?cursor=bm90LWEtY3Vyc29y")
        .session(&root)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .assert_problem_type(&support::problem("invalid-cursor"));

    // Promotion takes effect on the next request.
    let promoted: AdminUser = client
        .patch(&format!("/api/v1/admin/users/{}", ada.id()))
        .bearer(&admin_token)
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(promoted.role, ApiRole::Admin);
    client
        .get("/api/v1/admin/users")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK);
    let demoted: AdminUser = client
        .patch(&format!("/api/v1/admin/users/{}", ada.id()))
        .session(&root)
        .json(&json!({ "role": "user" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(demoted.role, ApiRole::User);
    client
        .get("/api/v1/admin/users")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::FORBIDDEN)
        .assert_problem_type(&support::problem("insufficient-scope"));

    // An administrator cannot demote or deactivate themselves, and restating
    // what is already true is not a change.
    for change in [json!({ "role": "user" }), json!({ "deactivated": true })] {
        client
            .patch(&format!("/api/v1/admin/users/{}", root.id()))
            .session(&root)
            .json(&change)
            .send()
            .await
            .assert_status(StatusCode::CONFLICT)
            .assert_problem_type(&support::problem("cannot-change-own-account"));
    }
    let same: AdminUser = client
        .patch(&format!("/api/v1/admin/users/{}", root.id()))
        .session(&root)
        .json(&json!({ "role": "admin", "deactivated": false }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(same.role, ApiRole::Admin);

    client
        .patch(&format!("/api/v1/admin/users/{}", Uuid::now_v7()))
        .session(&root)
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("user-not-found"));
    client
        .patch(&format!("/api/v1/admin/users/{}", ada.id()))
        .session(&root)
        .json(&json!({ "role": "owner" }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    client.assert_conformance();
}

#[tokio::test]
async fn deactivation_ends_sessions_and_refuses_credentials() {
    let env = env!();
    let client = env.client();
    let root = env.user(Role::Admin).await;
    let ada = env.user(Role::User).await;
    let auth = env.auth(&env.db.db);

    client
        .get("/api/v1/me")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::OK);
    let deactivated: AdminUser = client
        .patch(&format!("/api/v1/admin/users/{}", ada.id()))
        .session(&root)
        .json(&json!({ "deactivated": true }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert!(deactivated.deactivated_at.is_some());

    // Every session is gone, so the cookie names nothing; the tokens are
    // genuine and refused.
    assert!(auth.list_sessions(ada.id()).await.unwrap().is_empty());
    client
        .get("/api/v1/me")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    for token in [&ada.read, &ada.read_write] {
        client
            .get("/api/v1/me")
            .bearer(token)
            .send()
            .await
            .assert_status(StatusCode::FORBIDDEN)
            .assert_problem_type(&support::problem("account-deactivated"));
    }

    // Deactivating again is a no-op that keeps the first timestamp.
    let again: AdminUser = client
        .patch(&format!("/api/v1/admin/users/{}", ada.id()))
        .session(&root)
        .json(&json!({ "deactivated": true }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(again.deactivated_at, deactivated.deactivated_at);

    // Reactivation lets the tokens back in.
    let back: AdminUser = client
        .patch(&format!("/api/v1/admin/users/{}", ada.id()))
        .session(&root)
        .json(&json!({ "deactivated": false }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(back.deactivated_at, None);
    client
        .get("/api/v1/me")
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK);

    client.assert_conformance();
}

#[tokio::test]
async fn jobs_list_filter_and_retry() {
    let env = env!();
    let client = env.client();
    let root = env.user(Role::Admin).await;
    let db = &env.db.db;

    let pending = jobs_repo::enqueue_job(
        db,
        jobs_repo::build_job(
            JobType::Notification,
            Some(json!({ "secret": "do-not-show-me" })),
            3,
        ),
    )
    .await
    .unwrap();
    let failed = jobs_repo::enqueue_job(db, jobs_repo::build_job(JobType::Import, None, 1))
        .await
        .unwrap();
    // Claim both (oldest `run_at` first), then fail the second for good and
    // put the first back.
    let claimed = jobs_repo::claim_due_jobs(db, 10).await.unwrap();
    assert_eq!(claimed.len(), 2);
    jobs_repo::mark_failed(db, failed.id, "the provider was down".into())
        .await
        .unwrap();
    jobs_repo::reschedule(db, pending.id, 3600, None)
        .await
        .unwrap();
    let done = jobs_repo::enqueue_job(db, jobs_repo::build_job(JobType::MetadataRefresh, None, 1))
        .await
        .unwrap();
    jobs_repo::mark_completed(db, done.id).await.unwrap();

    let all = client.get("/api/v1/admin/jobs").session(&root).send().await;
    all.assert_status(StatusCode::OK);
    assert!(!all.text().contains("do-not-show-me"));
    let all: Page<Job> = all.json();
    assert_eq!(
        all.items.iter().map(|j| j.id).collect::<Vec<_>>(),
        [done.id, failed.id, pending.id]
    );
    let only_failed: Page<Job> = client
        .get("/api/v1/admin/jobs?status=failed")
        .bearer(root.admin.as_ref().unwrap())
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(only_failed.items.len(), 1);
    let shown = &only_failed.items[0];
    assert_eq!(shown.id, failed.id);
    assert_eq!(shown.status, JobStatus::Failed);
    assert_eq!(shown.attempts, 1);
    assert_eq!(shown.error.as_deref(), Some("the provider was down"));

    // The type filter is the server's, alone or beside the status one.
    let of_type = |query: &'static str| {
        let client = &client;
        let root = &root;
        async move {
            client
                .get(&format!("/api/v1/admin/jobs?{query}"))
                .session(root)
                .send()
                .await
                .assert_status(StatusCode::OK)
                .json::<Page<Job>>()
                .items
                .iter()
                .map(|j| j.id)
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(of_type("type=notification").await, [pending.id]);
    assert_eq!(of_type("type=metadata_refresh").await, [done.id]);
    assert_eq!(of_type("type=import&status=failed").await, [failed.id]);
    assert!(of_type("type=import&status=completed").await.is_empty());
    client
        .get("/api/v1/admin/jobs?type=cleanup")
        .session(&root)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    let mut ids = HashSet::new();
    let first: Page<Job> = client
        .get("/api/v1/admin/jobs?limit=2")
        .session(&root)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    ids.extend(first.items.iter().map(|j| j.id));
    let next = first.next_cursor.expect("a second page");
    let second: Page<Job> = client
        .get(&format!("/api/v1/admin/jobs?limit=2&cursor={next}"))
        .session(&root)
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json();
    assert_eq!(second.next_cursor, None);
    ids.extend(second.items.iter().map(|j| j.id));
    assert_eq!(ids.len(), 3);
    client
        .get("/api/v1/admin/jobs?status=stuck")
        .session(&root)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // A failed job goes back to the queue, due now, with a fresh budget.
    let retried = client
        .post(&format!("/api/v1/admin/jobs/{}/retries", failed.id))
        .session(&root)
        .send()
        .await;
    retried.assert_status(StatusCode::ACCEPTED);
    let retried: Job = retried.json();
    assert_eq!(retried.status, JobStatus::Pending);
    assert_eq!(retried.attempts, 0);
    assert_eq!(retried.error, None);
    let claimed = jobs_repo::claim_due_jobs(db, 10).await.unwrap();
    assert_eq!(
        claimed.iter().map(|j| j.id).collect::<Vec<_>>(),
        [failed.id]
    );

    // Anything not failed is not retried.
    for id in [failed.id, pending.id, done.id] {
        client
            .post(&format!("/api/v1/admin/jobs/{id}/retries"))
            .session(&root)
            .send()
            .await
            .assert_status(StatusCode::CONFLICT)
            .assert_problem_type(&support::problem("job-not-retryable"));
    }
    client
        .post(&format!("/api/v1/admin/jobs/{}/retries", Uuid::now_v7()))
        .session(&root)
        .send()
        .await
        .assert_status(StatusCode::NOT_FOUND)
        .assert_problem_type(&support::problem("job-not-found"));

    // The account statuses the sweep cannot reach, in this environment.
    client
        .patch(&format!("/api/v1/admin/users/{}", root.id()))
        .session(&root)
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .patch(&format!("/api/v1/admin/users/{}", root.id()))
        .session(&root)
        .json(&json!({ "deactivated": true }))
        .send()
        .await
        .assert_status(StatusCode::CONFLICT);

    client.assert_conformance();

    support::sweep::sweep(&env, "admin").await;
    env.assert_covered("admin");
}
