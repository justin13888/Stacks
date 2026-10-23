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
