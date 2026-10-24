//! The job queue claims with `FOR UPDATE SKIP LOCKED`: concurrent claimers
//! never receive the same job.

mod common;

use std::collections::HashSet;

use stacks_core::entity::enums::{JobStatus, JobType};
use stacks_core::jobs::repo;

#[tokio::test]
async fn concurrent_claimers_never_share_a_job() {
    let t = test_db!();
    const JOBS: usize = 200;
    for _ in 0..JOBS {
        repo::enqueue_job(&t.db, repo::build_job(JobType::Notification, None, 3))
            .await
            .unwrap();
    }

    // Eight claimers on two independent pools, racing in small batches.
    let pools = [t.db.clone(), t.second_pool().await];
    let mut tasks = Vec::new();
    for i in 0..8 {
        let db = pools[i % 2].clone();
        tasks.push(tokio::spawn(async move {
            let mut mine = Vec::new();
            loop {
                let batch = repo::claim_due_jobs(&db, 7).await.unwrap();
                if batch.is_empty() {
                    break;
                }
                for j in &batch {
                    assert_eq!(j.status, JobStatus::Running);
                    assert_eq!(j.attempts, 1);
                }
                mine.extend(batch.into_iter().map(|j| j.id));
                tokio::task::yield_now().await;
            }
            mine
        }));
    }
    let mut seen = HashSet::new();
    let mut total = 0;
    for task in tasks {
        for id in task.await.unwrap() {
            total += 1;
            assert!(seen.insert(id), "job {id} was claimed twice");
        }
    }
    assert_eq!(total, JOBS);
    assert!(repo::claim_due_jobs(&t.db, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn only_due_pending_jobs_are_claimed() {
    let t = test_db!();
    let later = repo::enqueue_job(&t.db, repo::build_job(JobType::Import, None, 3))
        .await
        .unwrap();
    repo::reschedule(&t.db, later.id, 3600, None).await.unwrap();
    let due = repo::enqueue_job(&t.db, repo::build_job(JobType::Import, None, 3))
        .await
        .unwrap();

    let claimed = repo::claim_due_jobs(&t.db, 10).await.unwrap();
    assert_eq!(
        claimed.iter().map(|j| j.id).collect::<Vec<_>>(),
        vec![due.id]
    );

    repo::mark_failed(&t.db, due.id, "boom".into())
        .await
        .unwrap();
    let retried = repo::retry_job(&t.db, due.id).await.unwrap();
    assert_eq!(retried.status, JobStatus::Pending);
    assert_eq!(retried.attempts, 0);
    assert_eq!(repo::claim_due_jobs(&t.db, 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn jobs_abandoned_mid_run_are_reclaimed() {
    use sea_orm::{ConnectionTrait, EntityTrait};
    use stacks_core::entity::job;

    let t = test_db!();
    let retryable = repo::enqueue_job(&t.db, repo::build_job(JobType::Import, None, 3))
        .await
        .unwrap();
    let spent = repo::enqueue_job(&t.db, repo::build_job(JobType::MetadataRefresh, None, 1))
        .await
        .unwrap();
    let live = repo::enqueue_job(&t.db, repo::build_job(JobType::Notification, None, 3))
        .await
        .unwrap();
    assert_eq!(repo::claim_due_jobs(&t.db, 10).await.unwrap().len(), 3);

    // The worker died an hour ago with two of them in flight; the third was
    // claimed just now by a live worker.
    t.db.execute_unprepared(&format!(
        "UPDATE jobs SET started_at = now() - interval '1 hour' WHERE id IN ('{}', '{}')",
        retryable.id, spent.id
    ))
    .await
    .unwrap();
    assert_eq!(
        repo::reclaim_stale_jobs(&t.db, 30 * 60)
            .await
            .unwrap()
            .len(),
        2
    );

    let get = |id| {
        let db = t.db.clone();
        async move { job::Entity::find_by_id(id).one(&db).await.unwrap().unwrap() }
    };
    let r = get(retryable.id).await;
    assert_eq!(
        r.status,
        JobStatus::Pending,
        "attempts remain: runnable again"
    );
    assert_eq!(r.attempts, 1, "the abandoned attempt still counts");
    assert!(r.started_at.is_none());
    assert!(r.error.as_deref().unwrap().contains("abandoned"));
    let s = get(spent.id).await;
    assert_eq!(s.status, JobStatus::Failed, "no attempts left");
    assert!(s.completed_at.is_some());
    assert_eq!(get(live.id).await.status, JobStatus::Running);

    // Only the retryable one comes back to a claimer, and nothing is
    // reclaimed twice.
    let claimed = repo::claim_due_jobs(&t.db, 10).await.unwrap();
    assert_eq!(
        claimed.iter().map(|j| j.id).collect::<Vec<_>>(),
        vec![retryable.id]
    );
    assert!(
        repo::reclaim_stale_jobs(&t.db, 30 * 60)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn only_a_failed_job_is_retried() {
    use stacks_core::jobs::JobError;

    let t = test_db!();
    let job = repo::enqueue_job(&t.db, repo::build_job(JobType::Import, None, 1))
        .await
        .unwrap();
    // Pending, then running: still queued, so not retried (a running job
    // reset under its worker would run twice).
    assert!(matches!(
        repo::retry_job(&t.db, job.id).await,
        Err(JobError::NotRetryable(JobStatus::Pending))
    ));
    repo::claim_due_jobs(&t.db, 10).await.unwrap();
    assert!(matches!(
        repo::retry_job(&t.db, job.id).await,
        Err(JobError::NotRetryable(JobStatus::Running))
    ));
    repo::mark_completed(&t.db, job.id).await.unwrap();
    assert!(matches!(
        repo::retry_job(&t.db, job.id).await,
        Err(JobError::NotRetryable(JobStatus::Completed))
    ));
    assert!(matches!(
        repo::retry_job(&t.db, uuid::Uuid::now_v7()).await,
        Err(JobError::NotFound)
    ));

    let failed = repo::enqueue_job(&t.db, repo::build_job(JobType::Import, None, 1))
        .await
        .unwrap();
    repo::claim_due_jobs(&t.db, 10).await.unwrap();
    repo::mark_failed(&t.db, failed.id, "boom".into())
        .await
        .unwrap();
    let retried = repo::retry_job(&t.db, failed.id).await.unwrap();
    assert_eq!(retried.status, JobStatus::Pending);
    assert_eq!(retried.attempts, 0);
    assert_eq!(retried.error, None);
    assert_eq!(retried.started_at, None);
    assert_eq!(retried.completed_at, None);
}

#[tokio::test]
async fn job_pages_are_newest_first_and_filter_by_status_and_type() {
    let t = test_db!();
    let mut ids = Vec::new();
    for _ in 0..5 {
        ids.push(
            repo::enqueue_job(&t.db, repo::build_job(JobType::Notification, None, 1))
                .await
                .unwrap()
                .id,
        );
    }
    repo::mark_failed(&t.db, ids[1], "x".into()).await.unwrap();
    ids.reverse();
    let all = repo::JobFilter::default();
    let first = repo::page_jobs(&t.db, all, None, 3).await.unwrap();
    assert_eq!(
        first.iter().map(|j| j.id).collect::<Vec<_>>(),
        ids[..3].to_vec()
    );
    let rest = repo::page_jobs(&t.db, all, Some(ids[2]), 3).await.unwrap();
    assert_eq!(
        rest.iter().map(|j| j.id).collect::<Vec<_>>(),
        ids[3..].to_vec()
    );
    let failed = repo::JobFilter {
        status: Some(JobStatus::Failed),
        ..all
    };
    let failed = repo::page_jobs(&t.db, failed, None, 10).await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, ids[3]);

    let import = repo::enqueue_job(&t.db, repo::build_job(JobType::Import, None, 1))
        .await
        .unwrap();
    let imports = repo::JobFilter {
        job_type: Some(JobType::Import),
        ..all
    };
    let imports = repo::page_jobs(&t.db, imports, None, 10).await.unwrap();
    assert_eq!(
        imports.iter().map(|j| j.id).collect::<Vec<_>>(),
        [import.id]
    );
    let failed_imports = repo::JobFilter {
        status: Some(JobStatus::Failed),
        job_type: Some(JobType::Import),
    };
    assert!(
        repo::page_jobs(&t.db, failed_imports, None, 10)
            .await
            .unwrap()
            .is_empty()
    );
}
