//! An import's rows on their own, with no metadata provider: what each
//! parser reads from its source's export, and the entry, runs and plays an
//! already-matched row becomes when applied against PostgreSQL 18. Matching,
//! the pipeline and outages are in `imports.rs`.

mod common;

use chrono::NaiveDate;
use stacks_core::entity::enums::{
    ConflictStrategy, DatePrecision, EntryStatus, ImportSource, PlaySource,
};
use stacks_core::import_export::ImportEntry;
use stacks_core::import_export::import::apply::{self, Applied};
use stacks_core::import_export::import::{mal, pipeline, tmdb};
use stacks_core::import_export::models::ImportDate;
use stacks_core::library::LibraryService;
use stacks_core::media::MediaKind;
use stacks_core::media::catalog;
use uuid::Uuid;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/import_export");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).expect("fixture exists")
}

const TMDB: &[u8] = br#"[{"tmdb_id": 550, "title": "Fight Club", "media_type": "movie"}]"#;

// ------------------------------------------------------------------ parsers

#[test]
fn the_mal_xml_fixture_reads_every_field_mal_gives() {
    let rows = mal::parse_mal_xml(&fixture("mal-animelist.xml")).unwrap();
    assert_eq!(rows.len(), 6);
    let eva = &rows[0];
    assert_eq!(eva.external_ids.mal_id.as_deref(), Some("30"));
    assert_eq!(eva.title.as_deref(), Some("Shinseiki Evangelion"));
    assert_eq!(eva.kind, Some(MediaKind::Show));
    assert_eq!(eva.total_episodes, Some(26));
    assert_eq!(eva.status.as_deref(), Some("completed"));
    assert_eq!(eva.score, Some(100));
    assert_eq!(eva.progress, Some(26));
    assert_eq!(eva.rewatch_count, Some(1));
    assert!(!eva.rewatching);
    assert_eq!(eva.notes.as_deref(), Some("Watched the Rebuilds after."));
    assert_eq!(
        eva.started.map(|d| (d.date, d.precision)),
        Some((
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
            DatePrecision::Day
        ))
    );
    let op = &rows[1];
    // MAL's 0 is "no score", and 0 episodes is "unknown".
    assert_eq!(op.score, None);
    assert_eq!(op.total_episodes, None);
    assert_eq!(op.notes, None);
    assert_eq!(op.started.map(|d| d.precision), Some(DatePrecision::Month));
    assert_eq!(op.completed, None, "0000-00-00 is no date");
    assert_eq!(
        rows[2].started.map(|d| d.precision),
        Some(DatePrecision::Year)
    );
    assert_eq!(rows[3].status.as_deref(), Some("dropped"));
    assert_eq!(rows[4].status.as_deref(), Some("planning"));
    assert_eq!(rows[5].kind, Some(MediaKind::Movie));
}

#[test]
fn the_mal_api_fixture_reads_every_field_mal_gives() {
    let rows = mal::parse_mal_json(&fixture("mal-animelist.json")).unwrap();
    assert_eq!(rows.len(), 4);
    let eva = &rows[0];
    assert_eq!(eva.title.as_deref(), Some("Neon Genesis Evangelion"));
    assert_eq!(eva.year, Some(1995));
    assert!(eva.rewatching);
    assert_eq!(eva.rewatch_count, Some(1));
    assert_eq!(eva.completed.map(|d| d.precision), Some(DatePrecision::Day));
    assert_eq!(
        rows[1].started.map(|d| d.precision),
        Some(DatePrecision::Month)
    );
    assert_eq!(rows[2].status.as_deref(), Some("paused"));
    // Several pages, or the bare `data` array, read the same.
    let doc: serde_json::Value = serde_json::from_slice(&fixture("mal-animelist.json")).unwrap();
    let pages = serde_json::json!([doc.clone(), doc.clone()]);
    assert_eq!(
        mal::parse_mal_json(pages.to_string().as_bytes())
            .unwrap()
            .len(),
        8
    );
    let bare = doc["data"].to_string();
    assert_eq!(mal::parse_mal_json(bare.as_bytes()).unwrap(), rows);
}

#[test]
fn each_tmdb_shape_reads() {
    let account = tmdb::parse_tmdb_json(&fixture("tmdb-account.json")).unwrap();
    assert_eq!(account.len(), 4, "a title on three lists is one row");
    let row = |tmdb_id: &str| {
        account
            .iter()
            .find(|r| r.external_ids.tmdb_id.as_deref() == Some(tmdb_id))
            .unwrap()
    };
    let fight_club = row("550");
    assert_eq!(fight_club.kind, Some(MediaKind::Movie));
    assert_eq!(fight_club.status.as_deref(), Some("completed"));
    assert_eq!(fight_club.score, Some(95));
    assert_eq!(fight_club.favourite, Some(true));
    assert_eq!(fight_club.year, Some(1999));
    let breaking_bad = row("1396");
    assert_eq!(breaking_bad.kind, Some(MediaKind::Show));
    assert_eq!(breaking_bad.score, Some(100), "v4's account_rating");
    assert_eq!(row("13").status.as_deref(), Some("planning"));
    assert_eq!(row("64010").kind, Some(MediaKind::Show));

    let page = tmdb::parse_tmdb_json(&fixture("tmdb-rated-page.json")).unwrap();
    assert_eq!(page.len(), 2);
    let rated = page.iter().find(|r| r.score.is_some()).unwrap();
    assert_eq!(
        (rated.status.as_deref(), rated.score),
        (Some("completed"), Some(80))
    );
    let unrated = page.iter().find(|r| r.score.is_none()).unwrap();
    assert_eq!(unrated.status.as_deref(), Some("planning"));

    let loose = tmdb::parse_tmdb_json(&fixture("tmdb-loose.json")).unwrap();
    assert_eq!(loose.len(), 3);
    assert_eq!(loose[1].kind, Some(MediaKind::Show));
}

// ------------------------------------------------------------------ apply rules

fn day(y: i32, m: u32, d: u32) -> Option<ImportDate> {
    Some(ImportDate {
        date: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
        precision: DatePrecision::Day,
    })
}

fn month(y: i32, m: u32) -> Option<ImportDate> {
    Some(ImportDate {
        date: NaiveDate::from_ymd_opt(y, m, 1).unwrap(),
        precision: DatePrecision::Month,
    })
}

fn row(status: Option<&str>) -> ImportEntry {
    ImportEntry {
        source: "mal".into(),
        status: status.map(str::to_owned),
        ..Default::default()
    }
}

/// An import of `user`'s that rows can be applied against directly.
async fn an_import(db: &sea_orm::DatabaseConnection, user: Uuid) -> Uuid {
    let file = TMDB.to_vec();
    let strategy = ConflictStrategy::Merge;
    pipeline::submit(
        db,
        user,
        ImportSource::TmdbJson,
        None,
        file,
        strategy,
        1 << 20,
    )
    .await
    .expect("the upload is accepted")
    .id
}

async fn apply_row(
    db: &sea_orm::DatabaseConnection,
    user: Uuid,
    media: &stacks_core::entity::media::Model,
    entry: &ImportEntry,
    strategy: ConflictStrategy,
    import_id: Uuid,
) {
    assert_eq!(
        apply::apply(db, user, media, entry, strategy, import_id)
            .await
            .unwrap(),
        Applied::Applied
    );
}

#[tokio::test]
async fn a_movie_row_is_one_play_dated_only_to_a_known_day() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let import_id = an_import(&t.db, ada.id).await;
    let library = LibraryService::new(t.db.clone());
    let merge = ConflictStrategy::Merge;

    // Completed on a known day: one play, at that day.
    let finished = common::movie(&t.db, 9001).await;
    let completed = ImportEntry {
        completed: day(2024, 3, 2),
        ..row(Some("completed"))
    };
    apply_row(&t.db, ada.id, &finished, &completed, merge, import_id).await;
    let plays = library.plays(ada.id, finished.id).await.unwrap();
    assert_eq!(plays.len(), 1);
    assert_eq!(
        plays[0].watched_at.map(|t| t.to_utc()),
        Some(
            NaiveDate::from_ymd_opt(2024, 3, 2)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc()
        )
    );
    assert_eq!(
        (plays[0].source, plays[0].import_id, plays[0].episode_id),
        (PlaySource::Import, Some(import_id), None)
    );
    // The same row again adds nothing.
    apply_row(&t.db, ada.id, &finished, &completed, merge, import_id).await;
    assert_eq!(library.plays(ada.id, finished.id).await.unwrap().len(), 1);

    // Completed in a known month only: the play is not given a day.
    let this_month = common::movie(&t.db, 9002).await;
    let entry = ImportEntry {
        completed: month(2024, 3),
        ..row(Some("completed"))
    };
    apply_row(&t.db, ada.id, &this_month, &entry, merge, import_id).await;
    let plays = library.plays(ada.id, this_month.id).await.unwrap();
    assert_eq!(plays.len(), 1);
    assert_eq!(plays[0].watched_at, None);

    // Watched (progress 1) but not completed: still one play.
    let watched = common::movie(&t.db, 9003).await;
    let entry = ImportEntry {
        progress: Some(1),
        ..row(Some("watching"))
    };
    apply_row(&t.db, ada.id, &watched, &entry, merge, import_id).await;
    assert_eq!(library.plays(ada.id, watched.id).await.unwrap().len(), 1);

    // Planned, with a start date that opens a run: no play.
    let planned = common::movie(&t.db, 9004).await;
    let entry = ImportEntry {
        started: day(2024, 3, 1),
        ..row(Some("planning"))
    };
    apply_row(&t.db, ada.id, &planned, &entry, merge, import_id).await;
    assert_eq!(library.runs(ada.id, planned.id).await.unwrap().len(), 1);
    assert!(library.plays(ada.id, planned.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_show_row_logs_season_one_episodes_and_never_a_special() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let import_id = an_import(&t.db, ada.id).await;
    let show = common::show(&t.db, 9101).await;
    let special = catalog::ensure_episode(&t.db, show.id, 0, 1).await.unwrap();
    let first = catalog::ensure_episode(&t.db, show.id, 1, 1).await.unwrap();
    let entry = ImportEntry {
        progress: Some(1),
        ..row(Some("watching"))
    };
    apply_row(
        &t.db,
        ada.id,
        &show,
        &entry,
        ConflictStrategy::Merge,
        import_id,
    )
    .await;
    let plays = LibraryService::new(t.db.clone())
        .plays(ada.id, show.id)
        .await
        .unwrap();
    assert_eq!(
        plays.iter().map(|p| p.episode_id).collect::<Vec<_>>(),
        [Some(first.id)],
        "not the special {}",
        special.id
    );
    assert_eq!(plays[0].watched_at, None, "an episode's day is not known");
}

#[tokio::test]
async fn merge_fills_only_what_the_entry_lacks_and_overwrite_replaces_it() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let import_id = an_import(&t.db, ada.id).await;
    let show = common::show(&t.db, 9201).await;
    let library = LibraryService::new(t.db.clone());
    let state = || async {
        let view = library.get(ada.id, show.id).await.unwrap().unwrap();
        let runs = library.runs(ada.id, show.id).await.unwrap();
        assert_eq!(runs.len(), 1, "one run throughout");
        (view.entry, runs.into_iter().next().unwrap())
    };

    // Watching since a known day; no score, notes, favourite or finish.
    let first = ImportEntry {
        started: day(2024, 1, 1),
        ..row(Some("watching"))
    };
    apply_row(
        &t.db,
        ada.id,
        &show,
        &first,
        ConflictStrategy::Merge,
        import_id,
    )
    .await;
    let (e, _) = state().await;
    assert_eq!(
        (e.score, e.favourite, e.notes.as_deref()),
        (None, false, None)
    );

    // Merge fills each blank, and keeps the status and start it had.
    let fuller = ImportEntry {
        score: Some(80),
        favourite: Some(true),
        notes: Some("from MAL".into()),
        started: day(2023, 6, 1),
        completed: day(2024, 2, 1),
        ..row(Some("completed"))
    };
    apply_row(
        &t.db,
        ada.id,
        &show,
        &fuller,
        ConflictStrategy::Merge,
        import_id,
    )
    .await;
    let (e, run) = state().await;
    assert_eq!(
        (e.status, e.score, e.favourite, e.notes.as_deref()),
        (EntryStatus::Watching, Some(80), true, Some("from MAL"))
    );
    assert_eq!(
        (run.started_on, run.completed_on),
        (
            NaiveDate::from_ymd_opt(2024, 1, 1),
            NaiveDate::from_ymd_opt(2024, 2, 1)
        )
    );

    // Merge never takes a value back: not the favourite, notes or score.
    let other = ImportEntry {
        score: Some(50),
        favourite: Some(false),
        notes: Some("someone else's".into()),
        started: day(2022, 1, 1),
        completed: day(2022, 2, 1),
        ..row(Some("dropped"))
    };
    apply_row(
        &t.db,
        ada.id,
        &show,
        &other,
        ConflictStrategy::Merge,
        import_id,
    )
    .await;
    assert_eq!(state().await, (e.clone(), run.clone()));

    // Overwrite takes every value the row gives, run dates included.
    let rewritten = ImportEntry {
        completed: month(2022, 2),
        notes: Some("rewritten".into()),
        ..other
    };
    apply_row(
        &t.db,
        ada.id,
        &show,
        &rewritten,
        ConflictStrategy::Overwrite,
        import_id,
    )
    .await;
    let (e, run) = state().await;
    assert_eq!(
        (e.status, e.score, e.favourite, e.notes.as_deref()),
        (EntryStatus::Dropped, Some(50), false, Some("rewritten"))
    );
    assert_eq!(
        (
            run.started_on,
            run.started_precision,
            run.completed_on,
            run.completed_precision
        ),
        (
            NaiveDate::from_ymd_opt(2022, 1, 1),
            Some(DatePrecision::Day),
            NaiveDate::from_ymd_opt(2022, 2, 1),
            Some(DatePrecision::Month)
        )
    );

    // A row that gives no dates leaves them, even under overwrite.
    apply_row(
        &t.db,
        ada.id,
        &show,
        &row(Some("dropped")),
        ConflictStrategy::Overwrite,
        import_id,
    )
    .await;
    assert_eq!(state().await.1, run);
}

#[tokio::test]
async fn a_row_opens_a_run_only_when_it_says_the_title_was_watched() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let import_id = an_import(&t.db, ada.id).await;
    let library = LibraryService::new(t.db.clone());
    let cases = [
        ("nothing", row(None), 0),
        ("planning", row(Some("planning")), 0),
        (
            "an episode",
            ImportEntry {
                progress: Some(1),
                ..row(None)
            },
            1,
        ),
        (
            "a start date",
            ImportEntry {
                started: day(2024, 1, 1),
                ..row(None)
            },
            1,
        ),
        (
            "a finish date",
            ImportEntry {
                completed: day(2024, 1, 1),
                ..row(None)
            },
            1,
        ),
        (
            "a rewatch",
            ImportEntry {
                rewatch_count: Some(1),
                ..row(None)
            },
            2,
        ),
        ("watching", row(Some("watching")), 1),
    ];
    for (n, (case, entry, runs)) in cases.into_iter().enumerate() {
        let show = common::show(&t.db, 9300 + n as i32).await;
        apply_row(
            &t.db,
            ada.id,
            &show,
            &entry,
            ConflictStrategy::Merge,
            import_id,
        )
        .await;
        assert_eq!(
            library.runs(ada.id, show.id).await.unwrap().len(),
            runs,
            "{case}"
        );
    }
}

#[tokio::test]
async fn a_new_entry_without_a_status_is_watching_once_an_episode_is_watched() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let import_id = an_import(&t.db, ada.id).await;
    let library = LibraryService::new(t.db.clone());
    for (n, (progress, status)) in [
        (Some(0), EntryStatus::Planning),
        (Some(1), EntryStatus::Watching),
    ]
    .into_iter()
    .enumerate()
    {
        let show = common::show(&t.db, 9400 + n as i32).await;
        let entry = ImportEntry {
            progress,
            ..row(None)
        };
        apply_row(
            &t.db,
            ada.id,
            &show,
            &entry,
            ConflictStrategy::Merge,
            import_id,
        )
        .await;
        let view = library.get(ada.id, show.id).await.unwrap().unwrap();
        assert_eq!(view.entry.status, status, "progress {progress:?}");
    }
}
