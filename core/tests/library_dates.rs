//! Dated changes against PostgreSQL 18: a play or a status change given the
//! day it happened dates the runs it touches in the same transaction, never
//! the server's today, and run dates that would be out of order are refused
//! with nothing written.

mod common;

use chrono::NaiveDate;
use stacks_core::Patch;
use stacks_core::entity::enums::{DatePrecision, EntryStatus, PlaySource};
use stacks_core::entity::watch_run;
use stacks_core::library::{
    EntryPatch, LibraryError, LibraryService, LogPlay, PlayBatch, RunDate, RunPatch,
};
use uuid::Uuid;

fn on(y: i32, m: u32, d: u32, precision: DatePrecision) -> RunDate {
    RunDate {
        date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
        precision,
    }
}

fn dates(run: &watch_run::Model) -> (Option<RunDate>, Option<RunDate>) {
    (RunDate::started(run), RunDate::completed(run))
}

fn completed() -> EntryPatch {
    EntryPatch {
        status: Some(EntryStatus::Completed),
        ..Default::default()
    }
}

async fn movie_on(
    svc: &LibraryService,
    user: Uuid,
    movie: &stacks_core::entity::media::Model,
    date: RunDate,
) -> Result<Vec<stacks_core::entity::play::Model>, LibraryError> {
    let at = date.date.and_hms_opt(12, 0, 0).unwrap().and_utc().into();
    svc.log_plays_on(
        user,
        movie,
        PlayBatch::Movie,
        Some(at),
        Some(date),
        PlaySource::Manual,
    )
    .await
}

#[tokio::test]
async fn a_dated_movie_play_opens_and_finishes_its_run_then() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await.id;
    let movie = common::movie(db, 1).await;
    let svc = LibraryService::new(db.clone());
    let march = on(2019, 3, 1, DatePrecision::Month);

    let plays = movie_on(&svc, user, &movie, march).await.unwrap();

    assert_eq!(plays.len(), 1);
    let view = svc.get(user, movie.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Completed);
    let runs = svc.runs(user, movie.id).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(dates(&runs[0]), (Some(march), Some(march)));
}

#[tokio::test]
async fn a_dated_play_moves_a_later_start_back_and_keeps_an_earlier_one() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await.id;
    let (early, late) = (common::movie(db, 1).await, common::movie(db, 2).await);
    let svc = LibraryService::new(db.clone());
    let day = |d| on(2020, 5, d, DatePrecision::Day);
    for movie in [&early, &late] {
        svc.upsert(
            user,
            movie.id,
            EntryPatch {
                status: Some(EntryStatus::Watching),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    let start = |movie: Uuid, d| {
        let svc = &svc;
        async move {
            let patch = RunPatch {
                started: Patch::Value(d),
                ..Default::default()
            };
            svc.update_run_by_number(user, movie, 1, patch)
                .await
                .unwrap();
        }
    };
    start(early.id, day(2)).await;
    start(late.id, day(20)).await;

    movie_on(&svc, user, &early, day(10)).await.unwrap();
    movie_on(&svc, user, &late, day(10)).await.unwrap();

    let early_runs = svc.runs(user, early.id).await.unwrap();
    assert_eq!(dates(&early_runs[0]), (Some(day(2)), Some(day(10))));
    let late_runs = svc.runs(user, late.id).await.unwrap();
    assert_eq!(dates(&late_runs[0]), (Some(day(10)), Some(day(10))));
}

#[tokio::test]
async fn a_dated_replay_of_a_finished_run_keeps_its_dates() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await.id;
    let movie = common::movie(db, 1).await;
    let svc = LibraryService::new(db.clone());
    let first = on(2018, 1, 1, DatePrecision::Year);
    movie_on(&svc, user, &movie, first).await.unwrap();

    let again = movie_on(&svc, user, &movie, on(2021, 7, 4, DatePrecision::Day))
        .await
        .unwrap();

    assert_eq!(again.len(), 1);
    let runs = svc.runs(user, movie.id).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(dates(&runs[0]), (Some(first), Some(first)));
    assert_eq!(svc.plays(user, movie.id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn completing_on_a_date_finishes_the_current_run_then() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await.id;
    let (show, other) = (common::show(db, 1).await, common::show(db, 2).await);
    let svc = LibraryService::new(db.clone());
    for title in [&show, &other] {
        svc.log_play(user, title, LogPlay::manual(Some((1, 1))))
            .await
            .unwrap();
        let patch = RunPatch {
            started: Patch::Value(on(2022, 2, 1, DatePrecision::Month)),
            ..Default::default()
        };
        svc.update_run_by_number(user, title.id, 1, patch)
            .await
            .unwrap();
    }
    let finished = on(2022, 4, 9, DatePrecision::Day);

    svc.patch_on(user, show.id, completed(), Some(finished))
        .await
        .unwrap();

    let view = svc.get(user, show.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Completed);
    let runs = svc.runs(user, show.id).await.unwrap();
    assert_eq!(runs[0].completed_on, Some(finished.date));

    // Already completed, but the run's completion was cleared: a dated
    // change still finishes it, and a second one overwrites nothing.
    svc.patch(user, other.id, completed()).await.unwrap();
    let patch = RunPatch {
        completed: Patch::Null,
        ..Default::default()
    };
    svc.update_run_by_number(user, other.id, 1, patch)
        .await
        .unwrap();
    svc.patch_on(user, other.id, completed(), Some(finished))
        .await
        .unwrap();
    svc.patch_on(
        user,
        other.id,
        completed(),
        Some(on(2023, 1, 1, DatePrecision::Year)),
    )
    .await
    .unwrap();
    let runs = svc.runs(user, other.id).await.unwrap();
    assert_eq!(RunDate::completed(&runs[0]), Some(finished));
}

#[tokio::test]
async fn completing_before_the_run_started_is_refused_and_changes_nothing() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await.id;
    let show = common::show(db, 1).await;
    let svc = LibraryService::new(db.clone());
    svc.log_play(user, &show, LogPlay::manual(Some((1, 1))))
        .await
        .unwrap();
    let start = on(2022, 6, 15, DatePrecision::Day);
    let patch = RunPatch {
        started: Patch::Value(start),
        ..Default::default()
    };
    svc.update_run_by_number(user, show.id, 1, patch)
        .await
        .unwrap();

    let refused = svc
        .patch_on(
            user,
            show.id,
            completed(),
            Some(on(2022, 6, 14, DatePrecision::Day)),
        )
        .await;

    assert!(matches!(refused, Err(LibraryError::RunDatesOutOfOrder)));
    let view = svc.get(user, show.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Watching);
    let runs = svc.runs(user, show.id).await.unwrap();
    assert_eq!(dates(&runs[0]), (Some(start), None));
}

#[tokio::test]
async fn a_run_cannot_finish_before_it_started() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await.id;
    let show = common::show(db, 1).await;
    let svc = LibraryService::new(db.clone());
    svc.log_play(user, &show, LogPlay::manual(Some((1, 1))))
        .await
        .unwrap();
    let set = |started, completed| RunPatch { started, completed };
    let june = on(2022, 6, 15, DatePrecision::Day);
    svc.update_run_by_number(user, show.id, 1, set(Patch::Value(june), Patch::Absent))
        .await
        .unwrap();

    let refused = svc
        .update_run_by_number(
            user,
            show.id,
            1,
            set(
                Patch::Absent,
                Patch::Value(on(2022, 5, 1, DatePrecision::Month)),
            ),
        )
        .await;
    assert!(matches!(refused, Err(LibraryError::RunDatesOutOfOrder)));

    // Known only to the year, 2022 does not contradict a start in June.
    let year = on(2022, 1, 1, DatePrecision::Year);
    let run = svc
        .update_run_by_number(user, show.id, 1, set(Patch::Absent, Patch::Value(year)))
        .await
        .unwrap();
    assert_eq!(dates(&run), (Some(june), Some(year)));
}

#[tokio::test]
async fn a_rewatch_cannot_start_before_the_last_run_finished() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await.id;
    let movie = common::movie(db, 1).await;
    let svc = LibraryService::new(db.clone());
    let finished = on(2021, 8, 10, DatePrecision::Day);
    movie_on(&svc, user, &movie, finished).await.unwrap();

    let refused = svc
        .start_rewatch(user, movie.id, Some(on(2021, 8, 9, DatePrecision::Day)))
        .await;

    assert!(matches!(refused, Err(LibraryError::RunDatesOutOfOrder)));
    assert_eq!(svc.runs(user, movie.id).await.unwrap().len(), 1);
    let view = svc.get(user, movie.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Completed);

    let same_month = on(2021, 8, 1, DatePrecision::Month);
    let run = svc
        .start_rewatch(user, movie.id, Some(same_month))
        .await
        .unwrap();
    assert_eq!(run.run_number, 2);
    assert_eq!(RunDate::started(&run), Some(same_month));
}
