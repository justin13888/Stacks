//! Library service behaviour against PostgreSQL 18: merge patches never lose
//! data, every mutation writes its event in the same transaction, deletes are
//! soft, plays are only voided, and progress is derived.

mod common;

use proptest::prelude::*;
use sea_orm::EntityTrait;
use stacks_core::Patch;
use stacks_core::entity::enums::{EntryEventKind, EntryStatus, PlaySource};
use stacks_core::entity::{entry_event, play};
use stacks_core::library::{EntryPatch, LibraryError, LibraryService, LogPlay};

fn status(s: EntryStatus) -> EntryPatch {
    EntryPatch {
        status: Some(s),
        ..Default::default()
    }
}

async fn kinds(db: &sea_orm::DatabaseConnection) -> Vec<EntryEventKind> {
    use sea_orm::QueryOrder;
    entry_event::Entity::find()
        .order_by_asc(entry_event::Column::Id)
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .collect()
}

/// The data-loss bug this schema replaces: changing status or score used to
/// reset notes, favourite, dates and rewatch count.
#[tokio::test]
async fn status_and_score_changes_keep_everything_else() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = common::show(db, 1).await;
    let svc = LibraryService::new(db.clone());

    svc.upsert(
        user.id,
        show.id,
        EntryPatch {
            status: Some(EntryStatus::Watching),
            score: Patch::Value(80),
            favourite: Some(true),
            notes: Patch::Value("keep me".into()),
        },
    )
    .await
    .unwrap();
    svc.log_play(user.id, &show, LogPlay::manual(Some((1, 1))))
        .await
        .unwrap();
    svc.log_play(user.id, &show, LogPlay::manual(Some((1, 2))))
        .await
        .unwrap();
    let runs_before = svc.runs(user.id, show.id).await.unwrap();

    svc.patch(user.id, show.id, status(EntryStatus::Paused))
        .await
        .unwrap();
    svc.patch(
        user.id,
        show.id,
        EntryPatch {
            score: Patch::Value(90),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let view = svc.get(user.id, show.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Paused);
    assert_eq!(view.entry.score, Some(90));
    assert!(view.entry.favourite);
    assert_eq!(view.entry.notes.as_deref(), Some("keep me"));
    assert_eq!(svc.runs(user.id, show.id).await.unwrap(), runs_before);
    let progress = view.progress.unwrap();
    assert_eq!(progress.current_run_episodes, 2);
    assert_eq!(progress.play_count, 2);

    // An explicit null clears; an absent field does not.
    svc.patch(
        user.id,
        show.id,
        EntryPatch {
            notes: Patch::Null,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let view = svc.get(user.id, show.id).await.unwrap().unwrap();
    assert_eq!(view.entry.notes, None);
    assert_eq!(view.entry.score, Some(90));
}

/// Every mutation path writes its event.
#[tokio::test]
async fn every_mutation_writes_an_event() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = common::show(db, 2).await;
    let svc = LibraryService::new(db.clone());

    svc.upsert(user.id, show.id, status(EntryStatus::Planning))
        .await
        .unwrap();
    let p = svc
        .log_play(user.id, &show, LogPlay::manual(Some((1, 1))))
        .await
        .unwrap();
    svc.void_play(user.id, p.id).await.unwrap();
    svc.start_rewatch(user.id, show.id, None).await.unwrap();
    svc.delete(user.id, show.id).await.unwrap();
    svc.restore(user.id, show.id).await.unwrap();

    use EntryEventKind::*;
    assert_eq!(
        kinds(db).await,
        vec![
            EntryCreated,
            // log_play on a planning entry: -> watching, run 1, the play
            EntryUpdated,
            RunStarted,
            PlayLogged,
            PlayVoided,
            // rewatch: run 2 (status already watching)
            RunStarted,
            EntryDeleted,
            EntryRestored,
        ]
    );
}

/// A failure writing the event rolls back the mutation too: they share one
/// transaction.
#[tokio::test]
async fn event_failure_rolls_back_the_mutation() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let movie = common::movie(db, 3).await;
    let svc = LibraryService::new(db.clone());
    svc.upsert(
        user.id,
        movie.id,
        EntryPatch {
            status: Some(EntryStatus::Planning),
            notes: Patch::Value("before".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let events_before = common::count(db, "entry_events").await;

    common::break_event_log(db).await;

    let err = svc
        .patch(
            user.id,
            movie.id,
            EntryPatch {
                notes: Patch::Value("after".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("event insert fails");
    assert!(matches!(err, LibraryError::Db(_)), "{err:?}");
    assert!(
        svc.log_play(user.id, &movie, LogPlay::manual(None))
            .await
            .is_err()
    );
    assert!(svc.delete(user.id, movie.id).await.is_err());
    let other = common::movie(db, 4).await;
    assert!(
        svc.upsert(user.id, other.id, status(EntryStatus::Planning))
            .await
            .is_err()
    );

    common::repair_event_log(db).await;

    let view = svc.get(user.id, movie.id).await.unwrap().unwrap();
    assert_eq!(view.entry.notes.as_deref(), Some("before"));
    assert_eq!(view.entry.status, EntryStatus::Planning);
    assert!(svc.get(user.id, other.id).await.unwrap().is_none());
    assert_eq!(common::count(db, "plays").await, 0);
    assert_eq!(common::count(db, "watch_runs").await, 0);
    assert_eq!(common::count(db, "entry_events").await, events_before);
}

#[tokio::test]
async fn soft_delete_and_restore() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let movie = common::movie(db, 5).await;
    let svc = LibraryService::new(db.clone());
    svc.upsert(
        user.id,
        movie.id,
        EntryPatch {
            status: Some(EntryStatus::Completed),
            score: Patch::Value(70),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    svc.delete(user.id, movie.id).await.unwrap();
    assert!(svc.get(user.id, movie.id).await.unwrap().is_none());
    let trash = svc.trash(user.id).await.unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].media.id, movie.id);
    assert!(matches!(
        svc.delete(user.id, movie.id).await,
        Err(LibraryError::NotFound)
    ));

    let restored = svc.restore(user.id, movie.id).await.unwrap();
    assert_eq!(restored.score, Some(70));
    assert_eq!(restored.status, EntryStatus::Completed);
    assert!(svc.trash(user.id).await.unwrap().is_empty());
    // The completion run survived the round trip.
    assert_eq!(svc.runs(user.id, movie.id).await.unwrap().len(), 1);

    // Re-adding a trashed title restores it instead of starting over.
    svc.delete(user.id, movie.id).await.unwrap();
    let again = svc
        .upsert(user.id, movie.id, EntryPatch::default())
        .await
        .unwrap();
    assert_eq!(again.id, restored.id);
    assert_eq!(again.score, Some(70));
}

#[tokio::test]
async fn plays_are_voided_never_deleted() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let movie = common::movie(db, 6).await;
    let svc = LibraryService::new(db.clone());
    let p = svc
        .log_play(user.id, &movie, LogPlay::manual(None))
        .await
        .unwrap();
    // A movie play completes the entry.
    let view = svc.get(user.id, movie.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Completed);
    assert!(
        svc.runs(user.id, movie.id).await.unwrap()[0]
            .completed_on
            .is_some()
    );

    let voided = svc.void_play(user.id, p.id).await.unwrap();
    assert!(voided.voided_at.is_some());
    assert!(matches!(
        svc.void_play(user.id, p.id).await,
        Err(LibraryError::AlreadyVoided)
    ));
    // Still there.
    let stored = play::Entity::find_by_id(p.id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.source, PlaySource::Manual);
    let progress = svc
        .get(user.id, movie.id)
        .await
        .unwrap()
        .unwrap()
        .progress
        .unwrap();
    assert_eq!(progress.play_count, 0);

    // Another user cannot void it.
    let stranger = common::user(db).await;
    assert!(matches!(
        svc.void_play(stranger.id, p.id).await,
        Err(LibraryError::PlayNotFound)
    ));
}

#[tokio::test]
async fn episodes_cannot_be_logged_against_a_movie() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let movie = common::movie(db, 7).await;
    let svc = LibraryService::new(db.clone());
    assert!(matches!(
        svc.log_play(user.id, &movie, LogPlay::manual(Some((1, 1))))
            .await,
        Err(LibraryError::NotAShow)
    ));
    assert!(svc.get(user.id, movie.id).await.unwrap().is_none());
}

#[tokio::test]
async fn unwatching_an_episode_voids_its_latest_play() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = common::show(db, 8).await;
    let svc = LibraryService::new(db.clone());
    svc.log_play(user.id, &show, LogPlay::manual(Some((1, 3))))
        .await
        .unwrap();
    assert!(
        svc.episode_play(user.id, show.id, 1, 3)
            .await
            .unwrap()
            .is_some()
    );
    let voided = svc
        .unwatch_episode(user.id, show.id, 1, 3)
        .await
        .unwrap()
        .expect("was watched");
    assert!(voided.voided_at.is_some());
    assert!(
        svc.episode_play(user.id, show.id, 1, 3)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        svc.unwatch_episode(user.id, show.id, 1, 3)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn rewatch_opens_a_new_run_and_progress_tracks_it() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = common::show(db, 9).await;
    let svc = LibraryService::new(db.clone());
    for ep in 1..=3 {
        svc.log_play(user.id, &show, LogPlay::manual(Some((1, ep))))
            .await
            .unwrap();
    }
    svc.patch(user.id, show.id, status(EntryStatus::Completed))
        .await
        .unwrap();
    let run2 = svc.start_rewatch(user.id, show.id, None).await.unwrap();
    assert_eq!(run2.run_number, 2);
    svc.log_play(user.id, &show, LogPlay::manual(Some((1, 1))))
        .await
        .unwrap();

    let view = svc.get(user.id, show.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Watching);
    let p = view.progress.unwrap();
    assert_eq!(p.run_count, 2);
    assert_eq!(p.rewatch_count, 1);
    assert_eq!(p.play_count, 4);
    assert_eq!(p.current_run_episodes, 1);
    assert!(p.first_watched_at.is_some() && p.last_watched_at >= p.first_watched_at);
}

#[derive(Debug, Clone)]
enum Op {
    Status(EntryStatus),
    Score(Option<i16>),
    Play(i32),
    Void(usize),
    Rewatch,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        prop_oneof![
            Just(EntryStatus::Planning),
            Just(EntryStatus::Watching),
            Just(EntryStatus::Completed),
            Just(EntryStatus::Paused),
            Just(EntryStatus::Dropped),
        ]
        .prop_map(Op::Status),
        proptest::option::of(0i16..=100).prop_map(Op::Score),
        (1i32..=6).prop_map(Op::Play),
        any::<usize>().prop_map(Op::Void),
        Just(Op::Rewatch),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    /// Any sequence of status, score, play, void and rewatch operations
    /// leaves the derived progress equal to what the plays say, and never
    /// loses the notes and favourite flag no operation touched.
    #[test]
    fn derived_progress_matches_the_plays(ops in proptest::collection::vec(op(), 1..25)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let Some(t) = common::TestDb::new().await else { return };
            let db = &t.db;
            let user = common::user(db).await;
            let show = common::show(db, 10).await;
            let svc = LibraryService::new(db.clone());
            svc.upsert(user.id, show.id, EntryPatch {
                status: Some(EntryStatus::Planning),
                favourite: Some(true),
                notes: Patch::Value("note".into()),
                ..Default::default()
            }).await.unwrap();

            // Model: per run, the live plays' episodes.
            let mut runs: Vec<Vec<(uuid::Uuid, i32, bool)>> = Vec::new();
            for op in ops {
                match op {
                    Op::Status(s) => { svc.patch(user.id, show.id, status(s)).await.unwrap(); }
                    Op::Score(s) => {
                        svc.patch(user.id, show.id, EntryPatch { score: Some(s).into(), ..Default::default() })
                            .await.unwrap();
                    }
                    Op::Play(ep) => {
                        let p = svc.log_play(user.id, &show, LogPlay::manual(Some((1, ep)))).await.unwrap();
                        let n = svc.runs(user.id, show.id).await.unwrap().len();
                        while runs.len() < n { runs.push(Vec::new()); }
                        runs[n - 1].push((p.id, ep, true));
                    }
                    Op::Void(i) => {
                        let live: Vec<(usize, usize)> = runs.iter().enumerate()
                            .flat_map(|(r, ps)| ps.iter().enumerate().filter(|(_, p)| p.2).map(move |(j, _)| (r, j)))
                            .collect();
                        if !live.is_empty() {
                            let (r, j) = live[i % live.len()];
                            svc.void_play(user.id, runs[r][j].0).await.unwrap();
                            runs[r][j].2 = false;
                        }
                    }
                    Op::Rewatch => { svc.start_rewatch(user.id, show.id, None).await.unwrap(); }
                }
                let n = svc.runs(user.id, show.id).await.unwrap().len();
                while runs.len() < n { runs.push(Vec::new()); }
            }

            let view = svc.get(user.id, show.id).await.unwrap().unwrap();
            let p = view.progress.unwrap();
            let live = |ps: &Vec<(uuid::Uuid, i32, bool)>| ps.iter().filter(|p| p.2).count() as i64;
            let total: i64 = runs.iter().map(live).sum();
            let current = runs.last().cloned().unwrap_or_default();
            let mut eps: Vec<i32> = current.iter().filter(|p| p.2).map(|p| p.1).collect();
            eps.sort();
            eps.dedup();
            assert_eq!(p.run_count as usize, runs.len());
            assert_eq!(p.rewatch_count as usize, runs.len().saturating_sub(1));
            assert_eq!(p.play_count, total);
            assert_eq!(p.current_run_plays, live(&current));
            assert_eq!(p.current_run_episodes, eps.len() as i64);
            assert!(view.entry.favourite);
            assert_eq!(view.entry.notes.as_deref(), Some("note"));
            // The log has an event for every play ever logged, voided or not.
            let logged = kinds(db).await.into_iter().filter(|k| *k == EntryEventKind::PlayLogged).count();
            assert_eq!(logged, runs.iter().map(Vec::len).sum::<usize>());
        });
    }
}
