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

// ------------------------------------------------------- keyset listings

use stacks_core::entity::enums::MediaKind;
use stacks_core::library::{EntryPosition, LibraryFilter, LibrarySort, PlayBatch};
use stacks_core::media::catalog;
use stacks_core::media::models::{EpisodeInfo, SeasonInfo};

/// Every window of a listing, followed to the end through its positions.
async fn walk(
    svc: &LibraryService,
    user: uuid::Uuid,
    filter: &LibraryFilter,
    sort: LibrarySort,
    limit: u64,
) -> Vec<uuid::Uuid> {
    let mut out = Vec::new();
    let mut after: Option<EntryPosition> = None;
    loop {
        let window = svc
            .list(user, filter, sort, after.as_ref(), limit)
            .await
            .unwrap();
        assert!(window.items.len() as u64 <= limit);
        out.extend(window.items.iter().map(|v| v.media.id));
        if !window.more {
            break;
        }
        let last = window.items.last().unwrap();
        after = Some(EntryPosition::of(sort, &last.entry, &last.media));
    }
    out
}

#[tokio::test]
async fn keyset_listing_pages_every_sort_without_repeats() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let other = common::user(db).await;
    let svc = LibraryService::new(db.clone());

    // Titles in mixed case, scores with ties and gaps.
    let specs: [(&str, Option<i16>); 7] = [
        ("delta", Some(70)),
        ("Alpha", None),
        ("charlie", Some(90)),
        ("Bravo", Some(70)),
        ("echo", None),
        ("Foxtrot", Some(10)),
        ("golf", Some(70)),
    ];
    let mut rows = Vec::new();
    for (i, (title, score)) in specs.iter().enumerate() {
        let mut snap = common::snapshot(MediaKind::Movie, 100 + i as i32, title);
        if *title == "echo" {
            snap.romaji_title = Some("Eko Romaji".into());
        }
        let m = catalog::upsert_snapshot(db, &snap).await.unwrap();
        svc.upsert(
            user.id,
            m.id,
            EntryPatch {
                status: Some(if i % 2 == 0 {
                    EntryStatus::Planning
                } else {
                    EntryStatus::Completed
                }),
                score: Some(*score).into(),
                favourite: Some(i % 3 == 0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        rows.push(m);
    }
    // Someone else's entry never shows.
    svc.upsert(other.id, rows[0].id, status(EntryStatus::Watching))
        .await
        .unwrap();
    // Touch the first entry so `updated` and `added` differ.
    svc.patch(
        user.id,
        rows[0].id,
        EntryPatch {
            notes: Patch::Value("touched".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let all = LibraryFilter::default();
    let ids = |order: &[usize]| order.iter().map(|i| rows[*i].id).collect::<Vec<_>>();

    // Added: newest first.
    assert_eq!(
        walk(&svc, user.id, &all, LibrarySort::Added, 3).await,
        ids(&[6, 5, 4, 3, 2, 1, 0])
    );
    // Updated: the touched one first, then newest.
    assert_eq!(
        walk(&svc, user.id, &all, LibrarySort::Updated, 2).await,
        ids(&[0, 6, 5, 4, 3, 2, 1])
    );
    // Title: A to Z ignoring case.
    assert_eq!(
        walk(&svc, user.id, &all, LibrarySort::Title, 3).await,
        ids(&[1, 3, 2, 0, 4, 5, 6])
    );
    // Score: highest first, ties by newest, unscored last; every window size.
    let by_score = ids(&[2, 6, 3, 0, 5, 4, 1]);
    for limit in 1..=8 {
        assert_eq!(
            walk(&svc, user.id, &all, LibrarySort::Score, limit).await,
            by_score,
            "window of {limit}"
        );
    }

    // Filters narrow, and still page.
    let completed = LibraryFilter {
        status: Some(EntryStatus::Completed),
        ..Default::default()
    };
    assert_eq!(
        walk(&svc, user.id, &completed, LibrarySort::Title, 1).await,
        ids(&[1, 3, 5])
    );
    let favourites = LibraryFilter {
        favourite: Some(true),
        ..Default::default()
    };
    assert_eq!(
        walk(&svc, user.id, &favourites, LibrarySort::Added, 1).await,
        ids(&[6, 3, 0])
    );
    let search = |q: &str| LibraryFilter {
        q: Some(q.into()),
        ..Default::default()
    };
    assert_eq!(
        walk(&svc, user.id, &search("ALPH"), LibrarySort::Title, 5).await,
        ids(&[1])
    );
    assert_eq!(
        walk(&svc, user.id, &search("romaji"), LibrarySort::Title, 5).await,
        ids(&[4])
    );
    assert!(
        walk(&svc, user.id, &search("%"), LibrarySort::Title, 5)
            .await
            .is_empty()
    );

    // The trash is its own listing.
    svc.delete(user.id, rows[2].id).await.unwrap();
    let trash = LibraryFilter {
        deleted: true,
        ..Default::default()
    };
    assert_eq!(
        walk(&svc, user.id, &trash, LibrarySort::Updated, 5).await,
        ids(&[2])
    );
    assert_eq!(
        walk(&svc, user.id, &all, LibrarySort::Title, 5).await,
        ids(&[1, 3, 0, 4, 5, 6])
    );
}

// ------------------------------------------------------------ bulk plays

/// Two seasons of three episodes; only season 1's episodes are stored.
async fn seasons(db: &sea_orm::DatabaseConnection, show: &stacks_core::entity::media::Model) {
    let season = |n: u32, air: &str| SeasonInfo {
        season_number: n,
        name: Some(format!("Season {n}")),
        overview: None,
        air_date: Some(air.into()),
        poster_url: None,
        episode_count: Some(3),
    };
    catalog::upsert_seasons(
        db,
        show.id,
        &[season(1, "2020-01-06"), season(2, "2030-01-06")],
    )
    .await
    .unwrap();
    let episodes: Vec<EpisodeInfo> = (1..=3)
        .map(|n| EpisodeInfo {
            season_number: 1,
            episode_number: n,
            name: Some(format!("Episode {n}")),
            overview: None,
            air_date: Some(format!("2020-01-0{}", 5 + n)),
            runtime_minutes: Some(45),
            still_url: None,
            rating: None,
        })
        .collect();
    catalog::upsert_season_episodes(db, show.id, &season(1, "2020-01-06"), &episodes)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_batch_of_plays_is_one_transaction() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let show = common::show(db, 200).await;
    let movie = common::movie(db, 201).await;
    let svc = LibraryService::new(db.clone());

    // The wrong kind for the title logs nothing.
    assert!(matches!(
        svc.log_plays(
            user.id,
            &show,
            PlayBatch::Movie,
            None,
            PlaySource::Manual,
            None
        )
        .await,
        Err(LibraryError::NotAMovie)
    ));
    assert!(matches!(
        svc.log_plays(
            user.id,
            &movie,
            PlayBatch::Episodes(vec![(1, 1)]),
            None,
            PlaySource::Manual,
            None
        )
        .await,
        Err(LibraryError::NotAShow)
    ));

    // A failure part-way rolls the whole batch back.
    common::break_event_log(db).await;
    assert!(
        svc.log_plays(
            user.id,
            &show,
            PlayBatch::Episodes(vec![(1, 1), (1, 2), (1, 3)]),
            None,
            PlaySource::Manual,
            None,
        )
        .await
        .is_err()
    );
    common::repair_event_log(db).await;
    assert_eq!(common::count(db, "plays").await, 0);
    assert_eq!(common::count(db, "library_entries").await, 0);
    assert_eq!(common::count(db, "watch_runs").await, 0);
    assert_eq!(common::count(db, "entry_events").await, 0);

    // A batch tracks the title, opens run 1 and logs every play.
    let logged = svc
        .log_plays(
            user.id,
            &show,
            PlayBatch::Episodes(vec![(1, 1), (1, 1), (1, 2)]),
            None,
            PlaySource::Manual,
            None,
        )
        .await
        .unwrap();
    assert_eq!(logged.len(), 3);
    let view = svc.get(user.id, show.id).await.unwrap().unwrap();
    assert_eq!(view.entry.status, EntryStatus::Watching);
    let p = view.progress.unwrap();
    assert_eq!((p.play_count, p.current_run_episodes), (3, 2));

    // Unwatched skips what the current run has seen.
    let fill = svc
        .log_plays(
            user.id,
            &show,
            PlayBatch::Unwatched(vec![(1, 1), (1, 2), (1, 3)]),
            None,
            PlaySource::Bulk,
            None,
        )
        .await
        .unwrap();
    assert_eq!(fill.len(), 1);
    assert_eq!(fill[0].source, PlaySource::Bulk);
    let events_before = common::count(db, "entry_events").await;
    let none = svc
        .log_plays(
            user.id,
            &show,
            PlayBatch::Unwatched(vec![(1, 1), (1, 3)]),
            None,
            PlaySource::Bulk,
            None,
        )
        .await
        .unwrap();
    assert!(none.is_empty());
    assert_eq!(common::count(db, "entry_events").await, events_before);
    // After a rewatch, the new run has seen nothing.
    svc.start_rewatch(user.id, show.id, None).await.unwrap();
    let again = svc
        .log_plays(
            user.id,
            &show,
            PlayBatch::Unwatched(vec![(1, 1), (1, 2)]),
            None,
            PlaySource::Bulk,
            None,
        )
        .await
        .unwrap();
    assert_eq!(again.len(), 2);
    let watched = svc.watched_in_current_run(view.entry.id).await.unwrap();
    assert_eq!(
        watched
            .iter()
            .map(|w| (w.season, w.episode))
            .collect::<Vec<_>>(),
        vec![(1, 1), (1, 2)]
    );

    // History reads back newest first, decoded, and pages by id.
    let first = svc.history(user.id, None, None, 4).await.unwrap();
    assert!(first.more);
    let ids: Vec<_> = first.items.iter().map(|d| d.event.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(ids, sorted);
    let last = *ids.last().unwrap();
    let rest = svc.history(user.id, None, Some(last), 1000).await.unwrap();
    assert!(!rest.more);
    assert!(rest.items.iter().all(|d| d.event.id < last));
    assert_eq!(
        (first.items.len() + rest.items.len()) as i64,
        common::count(db, "entry_events").await
    );
    match &first.items[0].change {
        stacks_core::library::events::Change::PlayLogged(p) => {
            assert_eq!(p.run, 2);
            assert_eq!(p.episode, Some((1, 2)));
            assert_eq!(p.source, PlaySource::Bulk);
        }
        other => panic!("expected the last play first, got {other:?}"),
    }
}

// --------------------------------------------------- up next and calendar

fn at(secs: i64) -> Option<sea_orm::prelude::DateTimeWithTimeZone> {
    chrono::DateTime::from_timestamp(1_700_000_000 + secs, 0).map(Into::into)
}

#[tokio::test]
async fn up_next_follows_the_furthest_episode_of_the_current_run() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let watched = common::show(db, 300).await;
    let fresh = common::show(db, 301).await;
    let dropped = common::show(db, 302).await;
    for show in [&watched, &fresh, &dropped] {
        seasons(db, show).await;
    }
    let svc = LibraryService::new(db.clone());
    let ep = |s, e| PlayBatch::Episodes(vec![(s, e)]);

    svc.log_plays(
        user.id,
        &watched,
        ep(1, 2),
        at(10),
        PlaySource::Manual,
        None,
    )
    .await
    .unwrap();
    svc.upsert(user.id, fresh.id, status(EntryStatus::Watching))
        .await
        .unwrap();
    svc.log_plays(
        user.id,
        &dropped,
        ep(1, 1),
        at(20),
        PlaySource::Manual,
        None,
    )
    .await
    .unwrap();
    svc.patch(user.id, dropped.id, status(EntryStatus::Dropped))
        .await
        .unwrap();

    let next = svc.up_next(user.id).await.unwrap();
    let summary: Vec<_> = next
        .iter()
        .map(|n| (n.media.id, n.season, n.number, n.episode.is_some()))
        .collect();
    // After S1E2 comes S1E3, even with S1E1 unwatched; a show never watched
    // starts at S1E1 and comes last; a dropped show is not listed.
    assert_eq!(
        summary,
        vec![(watched.id, 1, 3, true), (fresh.id, 1, 1, true)]
    );

    // Past the stored episodes, the season summary's count still says what
    // comes next.
    svc.log_plays(
        user.id,
        &watched,
        ep(1, 3),
        at(30),
        PlaySource::Manual,
        None,
    )
    .await
    .unwrap();
    let next = svc.up_next(user.id).await.unwrap();
    assert_eq!(
        (
            next[0].media.id,
            next[0].season,
            next[0].number,
            next[0].episode.is_some()
        ),
        (watched.id, 2, 1, false)
    );
    assert_eq!(next[0].last_watched_at, at(30));

    // Nothing left: not listed.
    svc.log_plays(
        user.id,
        &watched,
        PlayBatch::Episodes(vec![(2, 1), (2, 2), (2, 3)]),
        at(40),
        PlaySource::Manual,
        None,
    )
    .await
    .unwrap();
    let next = svc.up_next(user.id).await.unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].media.id, fresh.id);
}

#[tokio::test]
async fn the_calendar_lists_stored_episodes_in_range() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let a = catalog::upsert_snapshot(db, &common::snapshot(MediaKind::Show, 400, "beta show"))
        .await
        .unwrap();
    let b = catalog::upsert_snapshot(db, &common::snapshot(MediaKind::Show, 401, "Alpha show"))
        .await
        .unwrap();
    let dropped = common::show(db, 402).await;
    let trashed = common::show(db, 403).await;
    let untracked = common::show(db, 404).await;
    for show in [&a, &b, &dropped, &trashed, &untracked] {
        seasons(db, show).await;
    }
    let svc = LibraryService::new(db.clone());
    svc.upsert(user.id, a.id, status(EntryStatus::Planning))
        .await
        .unwrap();
    svc.log_plays(
        user.id,
        &b,
        PlayBatch::Episodes(vec![(1, 2)]),
        None,
        PlaySource::Manual,
        None,
    )
    .await
    .unwrap();
    svc.upsert(user.id, dropped.id, status(EntryStatus::Dropped))
        .await
        .unwrap();
    svc.upsert(user.id, trashed.id, status(EntryStatus::Watching))
        .await
        .unwrap();
    svc.delete(user.id, trashed.id).await.unwrap();

    let day = |d| chrono::NaiveDate::from_ymd_opt(2020, 1, d).unwrap();
    let items = svc.calendar(user.id, day(7), day(8)).await.unwrap();
    let got: Vec<_> = items
        .iter()
        .map(|i| (i.air_date, i.media.id, i.episode.number, i.watched))
        .collect();
    // S1E2 airs on the 7th and S1E3 on the 8th; within a day, by title.
    assert_eq!(
        got,
        vec![
            (day(7), b.id, 2, true),
            (day(7), a.id, 2, false),
            (day(8), b.id, 3, false),
            (day(8), a.id, 3, false),
        ]
    );
    assert!(
        svc.calendar(user.id, day(20), day(25))
            .await
            .unwrap()
            .is_empty()
    );
}
