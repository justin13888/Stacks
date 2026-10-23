//! The lossless backup: export → restore round trips, the restore modes, dry
//! runs, version handling and secrets.

mod common;

use std::collections::HashMap;

use chrono::NaiveDate;
use proptest::prelude::*;
use serde_json::{Value, json};
use stacks_core::Patch;
use stacks_core::blocklist::BlocklistService;
use stacks_core::entity::enums::{
    ChannelType, DatePrecision, EntryStatus, NotificationEventType, PlaySource,
};
use stacks_core::entity::media;
use stacks_core::import_export::backup::{
    self, Backup, ExportOptions, ReadError, RestoreError, RestoreMode, RestoreOptions,
};
use stacks_core::library::{EntryPatch, LibraryService, PlayBatch, RunDate, RunPatch};
use stacks_core::lists::ListService;
use stacks_core::lists::service::CreateListRequest;
use stacks_core::notifications::NotificationService;
use stacks_core::notifications::service::NewChannel;
use stacks_core::tags::TagService;
use stacks_core::user::UserService;
use uuid::Uuid;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/import_export");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).expect("fixture exists")
}

/// Every table a restore can write, for "nothing changed" checks.
const TABLES: &[&str] = &[
    "media",
    "media_refs",
    "seasons",
    "episodes",
    "library_entries",
    "watch_runs",
    "plays",
    "entry_events",
    "lists",
    "list_items",
    "tags",
    "entry_tags",
    "blocklist",
    "notification_channels",
    "notification_subscriptions",
];

async fn counts(db: &sea_orm::DatabaseConnection) -> Vec<i64> {
    let mut out = Vec::new();
    for t in TABLES {
        out.push(common::count(db, t).await);
    }
    out
}

async fn export(db: &sea_orm::DatabaseConnection, user: Uuid) -> Backup {
    backup::export_backup(
        db,
        user,
        ExportOptions {
            include_secrets: true,
        },
    )
    .await
    .expect("export")
}

fn bytes(b: &Backup) -> Vec<u8> {
    serde_json::to_vec(b).expect("a backup serializes")
}

/// A backup as comparable JSON: no export time or profile, the owner's id
/// written as `<user>`, and each episode id (which the catalog assigns, so
/// it differs between databases) as `<title>/<season>/<number>`.
fn canonical(b: &Backup) -> Value {
    let mut b = b.clone();
    let owner = b.user.take().map(|u| u.id.to_string());
    b.exported_at = chrono::DateTime::UNIX_EPOCH;
    let mut names: HashMap<String, String> = HashMap::new();
    if let Some(owner) = owner {
        names.insert(owner, "<user>".into());
    }
    for e in &b.entries {
        for r in &e.runs {
            for p in &r.plays {
                if let Some(ep) = &p.episode {
                    names.insert(
                        ep.id.to_string(),
                        format!("{}/{}/{}", e.title_id, ep.season, ep.number),
                    );
                }
            }
        }
    }
    let mut v = serde_json::to_value(b).expect("a backup serializes");
    fn rename(v: &mut Value, names: &HashMap<String, String>) {
        match v {
            Value::String(s) => {
                if let Some(n) = names.get(s.as_str()) {
                    *s = n.clone();
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| rename(x, names)),
            Value::Object(m) => m.values_mut().for_each(|x| rename(x, names)),
            _ => {}
        }
    }
    rename(&mut v, &names);
    v
}

// ------------------------------------------------------------ a dataset

/// The titles a generated dataset draws on: three movies and three shows.
async fn titles(db: &sea_orm::DatabaseConnection) -> Vec<media::Model> {
    let mut out = Vec::new();
    for n in 0..3 {
        out.push(common::movie(db, 500 + n).await);
        out.push(common::show(db, 1300 + n).await);
    }
    out
}

#[derive(Debug, Clone)]
enum Op {
    Track {
        title: usize,
        status: EntryStatus,
        score: Option<i16>,
        favourite: bool,
        notes: Option<String>,
    },
    Play {
        title: usize,
        season: i32,
        episode: i32,
    },
    Void(usize),
    Delete(usize),
    Restore(usize),
    Rewatch(usize),
    Dates {
        title: usize,
        day: u32,
        precision: DatePrecision,
    },
    List(String),
    ListItem {
        list: usize,
        title: usize,
    },
    Tag(String),
    Assign {
        tag: usize,
        title: usize,
    },
    Block(usize),
    Channel,
    Settings(&'static str),
}

fn status() -> impl Strategy<Value = EntryStatus> {
    prop_oneof![
        Just(EntryStatus::Planning),
        Just(EntryStatus::Watching),
        Just(EntryStatus::Completed),
        Just(EntryStatus::Paused),
        Just(EntryStatus::Dropped),
    ]
}

fn precision() -> impl Strategy<Value = DatePrecision> {
    prop_oneof![
        Just(DatePrecision::Day),
        Just(DatePrecision::Month),
        Just(DatePrecision::Year),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    let title = 0usize..6;
    prop_oneof![
        4 => (
            title.clone(),
            status(),
            proptest::option::of(0i16..=100),
            any::<bool>(),
            proptest::option::of("[a-z ]{0,12}"),
        )
            .prop_map(|(title, status, score, favourite, notes)| Op::Track {
                title,
                status,
                score,
                favourite,
                notes,
            }),
        4 => (title.clone(), 1i32..=2, 1i32..=4)
            .prop_map(|(title, season, episode)| Op::Play { title, season, episode }),
        2 => any::<usize>().prop_map(Op::Void),
        1 => title.clone().prop_map(Op::Delete),
        1 => title.clone().prop_map(Op::Restore),
        1 => title.clone().prop_map(Op::Rewatch),
        1 => (title.clone(), 1u32..=28, precision())
            .prop_map(|(title, day, precision)| Op::Dates { title, day, precision }),
        1 => "[a-z]{1,8}".prop_map(Op::List),
        2 => (0usize..4, title.clone()).prop_map(|(list, title)| Op::ListItem { list, title }),
        1 => "[a-z]{1,8}".prop_map(Op::Tag),
        2 => (0usize..4, title.clone()).prop_map(|(tag, title)| Op::Assign { tag, title }),
        1 => title.clone().prop_map(Op::Block),
        1 => Just(Op::Channel),
        1 => prop_oneof![Just("point_10"), Just("point_5"), Just("point_100")].prop_map(Op::Settings),
    ]
}

/// Apply generated operations through the services, as a user would.
/// Operations that do not apply (voiding with no plays, tagging an untracked
/// title) are refused by the services and skipped.
async fn apply(db: &sea_orm::DatabaseConnection, user: Uuid, titles: &[media::Model], ops: &[Op]) {
    let library = LibraryService::new(db.clone());
    let lists = ListService::new(db.clone());
    let tags = TagService::new(db.clone());
    let mut list_ids = Vec::new();
    let mut tag_ids = Vec::new();
    for op in ops {
        match op {
            Op::Track {
                title,
                status,
                score,
                favourite,
                notes,
            } => {
                let _ = library
                    .upsert(
                        user,
                        titles[*title].id,
                        EntryPatch {
                            status: Some(*status),
                            score: Some(*score).into(),
                            favourite: Some(*favourite),
                            notes: Some(notes.clone()).into(),
                        },
                    )
                    .await;
            }
            Op::Play {
                title,
                season,
                episode,
            } => {
                let m = &titles[*title];
                let batch = match m.kind {
                    stacks_core::media::MediaKind::Movie => PlayBatch::Movie,
                    stacks_core::media::MediaKind::Show => {
                        PlayBatch::Episodes(vec![(*season, *episode)])
                    }
                };
                let _ = library
                    .log_plays(
                        user,
                        m,
                        batch,
                        Some(stacks_core::db::now()),
                        PlaySource::Manual,
                        None,
                    )
                    .await;
            }
            Op::Void(i) => {
                let mut live = Vec::new();
                for m in titles {
                    if let Ok(plays) = library.plays(user, m.id).await {
                        live.extend(plays.into_iter().filter(|p| p.voided_at.is_none()));
                    }
                }
                if !live.is_empty() {
                    let _ = library.void_play(user, live[i % live.len()].id).await;
                }
            }
            Op::Delete(t) => {
                let _ = library.delete(user, titles[*t].id).await;
            }
            Op::Restore(t) => {
                let _ = library.restore(user, titles[*t].id).await;
            }
            Op::Rewatch(t) => {
                let _ = library.start_rewatch(user, titles[*t].id, None).await;
            }
            Op::Dates {
                title,
                day,
                precision,
            } => {
                let date = RunDate {
                    date: NaiveDate::from_ymd_opt(2023, 5, *day).expect("a date"),
                    precision: *precision,
                };
                let _ = library
                    .update_run_by_number(
                        user,
                        titles[*title].id,
                        1,
                        RunPatch {
                            started: Patch::Value(date),
                            ..Default::default()
                        },
                    )
                    .await;
            }
            Op::List(name) => {
                if let Ok(l) = lists
                    .create_list(
                        user,
                        CreateListRequest {
                            name: name.clone(),
                            description: Some(format!("{name}, described")),
                            is_smart: false,
                            filter_query: None,
                        },
                    )
                    .await
                {
                    list_ids.push(l.id);
                }
            }
            Op::ListItem { list, title } => {
                if let Some(l) = list_ids.get(*list) {
                    let _ = lists
                        .add_item(user, *l, titles[*title].id, Some("a note".into()))
                        .await;
                }
            }
            Op::Tag(name) => {
                if let Ok(t) = tags
                    .create_tag(user, name.clone(), Some("#aabbcc".into()))
                    .await
                {
                    tag_ids.push(t.id);
                }
            }
            Op::Assign { tag, title } => {
                if let Some(t) = tag_ids.get(*tag) {
                    let _ = tags.assign_tag(user, titles[*title].id, *t).await;
                }
            }
            Op::Block(t) => {
                let _ = BlocklistService::new(db.clone())
                    .block(user, titles[*t].id)
                    .await;
            }
            Op::Channel => {
                let n = NotificationService::new(db.clone());
                if let Ok(c) = n
                    .create_channel(
                        user,
                        NewChannel {
                            channel_type: ChannelType::Webhook,
                            name: "hook".into(),
                            enabled: true,
                            config: json!({"url": "https://hooks.example.com/stacks", "secret": "s3cret"}),
                        },
                    )
                    .await
                {
                    let _ = n
                        .create_subscription(user, c.id, NotificationEventType::NewEpisode, true)
                        .await;
                }
            }
            Op::Settings(format) => {
                let _ = UserService::new(db.clone())
                    .update_settings(user, json!({ "score_format": format }))
                    .await;
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

    /// For any dataset the services can build, restoring its backup into a
    /// fresh account on a fresh server and backing that up again gives the
    /// same document: every entry field, run, play (voided ones included),
    /// event, list order, tag, assignment, blocklist row, channel and
    /// setting, with the same ids and timestamps.
    #[test]
    fn restore_of_an_export_is_the_same_dataset(ops in proptest::collection::vec(op(), 1..40)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let Some(source) = common::TestDb::new().await else { return };
            let Some(target) = common::TestDb::new().await else { return };
            let ada = common::user(&source.db).await;
            let pool = titles(&source.db).await;
            apply(&source.db, ada.id, &pool, &ops).await;
            let before = export(&source.db, ada.id).await;

            let eve = common::user(&target.db).await;
            let report = backup::restore(&target.db, eve.id, &bytes(&before), RestoreOptions::default())
                .await
                .expect("the restore succeeds");
            assert!(report.warnings.is_empty(), "{:?}", report.warnings);
            assert_eq!(report.entries.inserted as usize, before.entries.len());
            let after = export(&target.db, eve.id).await;
            assert_eq!(canonical(&after), canonical(&before));

            // The restored history reads back like any other.
            LibraryService::new(target.db.clone())
                .history(eve.id, None, None, 1000)
                .await
                .expect("the restored history decodes");
        });
    }
}

/// A small fixed dataset: a show watched twice with a voided play, a movie
/// in the trash, a list, a tag, a blocked title and a webhook channel.
async fn fixed_dataset(db: &sea_orm::DatabaseConnection, user: Uuid) -> Vec<media::Model> {
    let pool = titles(db).await;
    let ops = vec![
        Op::Track {
            title: 1,
            status: EntryStatus::Watching,
            score: Some(80),
            favourite: false,
            notes: Some("great".into()),
        },
        Op::Play {
            title: 1,
            season: 1,
            episode: 1,
        },
        Op::Play {
            title: 1,
            season: 1,
            episode: 2,
        },
        Op::Void(1),
        Op::Rewatch(1),
        Op::Play {
            title: 1,
            season: 1,
            episode: 1,
        },
        Op::Play {
            title: 0,
            season: 1,
            episode: 1,
        },
        Op::Delete(0),
        Op::Track {
            title: 2,
            status: EntryStatus::Planning,
            score: None,
            favourite: true,
            notes: None,
        },
        Op::List("faves".into()),
        Op::ListItem { list: 0, title: 1 },
        Op::ListItem { list: 0, title: 2 },
        Op::Tag("cozy".into()),
        Op::Assign { tag: 0, title: 1 },
        Op::Block(5),
        Op::Channel,
        Op::Settings("point_10"),
    ];
    apply(db, user, &pool, &ops).await;
    pool
}

#[tokio::test]
async fn a_backup_restores_into_a_second_account_on_the_same_server() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    fixed_dataset(&t.db, ada.id).await;
    let original = export(&t.db, ada.id).await;
    let eve = common::user(&t.db).await;
    let report = backup::restore(&t.db, eve.id, &bytes(&original), RestoreOptions::default())
        .await
        .unwrap();
    assert_eq!(report.titles.matched as usize, original.titles.len());
    assert_eq!(report.titles.created, 0);
    let restored = export(&t.db, eve.id).await;

    // Every id was taken by ada's rows, so every row got a new one...
    assert_eq!(restored.entries.len(), original.entries.len());
    for (a, b) in original.entries.iter().zip(&restored.entries) {
        assert_ne!(a.id, b.id);
        assert_eq!(a.title_id, b.title_id, "the same catalog titles");
        assert_eq!(
            (a.status, a.score, a.favourite, &a.notes, a.deleted_at),
            (b.status, b.score, b.favourite, &b.notes, b.deleted_at)
        );
        assert_eq!(a.runs.len(), b.runs.len());
        assert_eq!(a.events.len(), b.events.len());
        assert_eq!(a.tags.len(), b.tags.len());
    }
    // ...and the event payloads follow the new ids, so the history decodes.
    let history = LibraryService::new(t.db.clone())
        .history(eve.id, None, None, 1000)
        .await
        .expect("eve's history decodes");
    assert_eq!(
        history.items.len(),
        original
            .entries
            .iter()
            .map(|e| e.events.len())
            .sum::<usize>()
    );
    let entry_ids: Vec<Uuid> = restored.entries.iter().map(|e| e.id).collect();
    assert!(
        history
            .items
            .iter()
            .all(|d| entry_ids.contains(&d.event.entry_id))
    );
    for d in &history.items {
        let payload = serde_json::to_string(&d.event.after).unwrap();
        assert!(
            !payload.contains(&ada.id.to_string()),
            "no payload names ada"
        );
    }
    // Ada's data is untouched.
    assert_eq!(
        canonical(&export(&t.db, ada.id).await),
        canonical(&original)
    );
}

#[tokio::test]
async fn restoring_the_same_backup_twice_changes_nothing_the_second_time() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    fixed_dataset(&t.db, ada.id).await;
    let original = bytes(&export(&t.db, ada.id).await);
    for mode in [
        RestoreMode::Merge,
        RestoreMode::Skip,
        RestoreMode::Overwrite,
    ] {
        let before = counts(&t.db).await;
        let report = backup::restore(
            &t.db,
            ada.id,
            &original,
            RestoreOptions {
                mode,
                dry_run: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(counts(&t.db).await, before, "{mode:?} added rows");
        assert_eq!(report.entries.inserted, 0);
        assert_eq!(report.entries.updated, 0, "{mode:?}");
        assert_eq!(
            report.plays.inserted + report.events.inserted,
            0,
            "{mode:?}"
        );
    }
}

/// The second account's rows all got new ids the first time; the second
/// restore still recognises every one of them.
#[tokio::test]
async fn restoring_twice_into_a_second_account_adds_nothing_the_second_time() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    fixed_dataset(&t.db, ada.id).await;
    let original = bytes(&export(&t.db, ada.id).await);
    let eve = common::user(&t.db).await;
    backup::restore(&t.db, eve.id, &original, RestoreOptions::default())
        .await
        .unwrap();
    let before = counts(&t.db).await;
    for mode in [
        RestoreMode::Merge,
        RestoreMode::Overwrite,
        RestoreMode::Skip,
    ] {
        let report = backup::restore(
            &t.db,
            eve.id,
            &original,
            RestoreOptions {
                mode,
                dry_run: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            counts(&t.db).await,
            before,
            "{mode:?} added rows: {report:?}"
        );
    }
}

/// Ada changed her mind after the backup: a new score, cleared notes, a
/// voided play the backup has live. Each mode then does what it says.
#[tokio::test]
async fn merge_skip_and_overwrite_treat_an_existing_entry_as_documented() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let pool = fixed_dataset(&t.db, ada.id).await;
    let show = &pool[1];
    let snapshot = export(&t.db, ada.id).await;
    let library = LibraryService::new(t.db.clone());

    let diverge = || async {
        library
            .patch(
                ada.id,
                show.id,
                EntryPatch {
                    score: Patch::Value(40),
                    notes: Patch::Null,
                    status: Some(EntryStatus::Paused),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    };
    let state = || async { library.get(ada.id, show.id).await.unwrap().unwrap().entry };

    // merge: the local score and status stay; the cleared notes are filled.
    diverge().await;
    let report = backup::restore(&t.db, ada.id, &bytes(&snapshot), RestoreOptions::default())
        .await
        .unwrap();
    let e = state().await;
    assert_eq!(
        (e.score, e.notes.as_deref(), e.status),
        (Some(40), Some("great"), EntryStatus::Paused)
    );
    assert_eq!(report.entries.updated, 1);

    // skip: nothing about an existing entry changes.
    diverge().await;
    library
        .patch(
            ada.id,
            show.id,
            EntryPatch {
                notes: Patch::Null,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let report = backup::restore(
        &t.db,
        ada.id,
        &bytes(&snapshot),
        RestoreOptions {
            mode: RestoreMode::Skip,
            dry_run: false,
        },
    )
    .await
    .unwrap();
    let e = state().await;
    assert_eq!((e.score, e.notes.as_deref()), (Some(40), None));
    assert_eq!(report.entries.updated, 0);
    assert_eq!(report.entries.skipped as usize, snapshot.entries.len());

    // overwrite: the backup wins.
    let events_before = common::count(&t.db, "entry_events").await;
    let report = backup::restore(
        &t.db,
        ada.id,
        &bytes(&snapshot),
        RestoreOptions {
            mode: RestoreMode::Overwrite,
            dry_run: false,
        },
    )
    .await
    .unwrap();
    let e = state().await;
    assert_eq!(
        (e.score, e.notes.as_deref(), e.status),
        (Some(80), Some("great"), EntryStatus::Watching)
    );
    assert_eq!(report.entries.updated, 1);
    assert!(
        common::count(&t.db, "entry_events").await > events_before,
        "the change is in the event log"
    );

    // A play voided after the backup stays voided under every mode: undo is
    // never undone by a restore.
    let live = library
        .plays(ada.id, show.id)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.voided_at.is_none())
        .unwrap();
    library.void_play(ada.id, live.id).await.unwrap();
    for mode in [RestoreMode::Merge, RestoreMode::Overwrite] {
        backup::restore(
            &t.db,
            ada.id,
            &bytes(&snapshot),
            RestoreOptions {
                mode,
                dry_run: false,
            },
        )
        .await
        .unwrap();
        let p = library
            .plays(ada.id, show.id)
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.id == live.id)
            .unwrap();
        assert!(p.voided_at.is_some(), "{mode:?} un-voided a play");
    }
}

#[tokio::test]
async fn merge_unions_plays_and_fills_a_missing_run_date() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let pool = titles(&t.db).await;
    let show = &pool[1];
    let library = LibraryService::new(t.db.clone());
    // The backup: episodes 1 and 2, and a start date on run 1.
    library
        .log_plays(
            ada.id,
            show,
            PlayBatch::Episodes(vec![(1, 1), (1, 2)]),
            None,
            PlaySource::Manual,
            None,
        )
        .await
        .unwrap();
    let started = RunDate {
        date: NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
        precision: DatePrecision::Month,
    };
    library
        .update_run_by_number(
            ada.id,
            show.id,
            1,
            RunPatch {
                started: Patch::Value(started),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let snapshot = export(&t.db, ada.id).await;

    // Into eve, who watched episode 3 of the same show and has no dates.
    let eve = common::user(&t.db).await;
    library
        .log_plays(
            eve.id,
            show,
            PlayBatch::Episodes(vec![(1, 3)]),
            None,
            PlaySource::Manual,
            None,
        )
        .await
        .unwrap();
    library
        .update_run_by_number(
            eve.id,
            show.id,
            1,
            RunPatch {
                started: Patch::Null,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let report = backup::restore(&t.db, eve.id, &bytes(&snapshot), RestoreOptions::default())
        .await
        .unwrap();
    assert_eq!(report.plays.inserted, 2);
    assert_eq!(report.runs.updated, 1);
    let watched: Vec<i32> = library
        .watched_in_current_run(
            library
                .get(eve.id, show.id)
                .await
                .unwrap()
                .unwrap()
                .entry
                .id,
        )
        .await
        .unwrap()
        .into_iter()
        .map(|w| w.episode)
        .collect();
    assert_eq!(watched, [1, 2, 3]);
    let runs = library.runs(eve.id, show.id).await.unwrap();
    assert_eq!(runs[0].started_on, Some(started.date));
    assert_eq!(runs[0].started_precision, Some(DatePrecision::Month));
}

#[tokio::test]
async fn a_dry_run_reports_the_restore_and_writes_nothing() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    fixed_dataset(&t.db, ada.id).await;
    let snapshot = bytes(&export(&t.db, ada.id).await);
    let eve = common::user(&t.db).await;
    let before = counts(&t.db).await;
    let dry = backup::restore(
        &t.db,
        eve.id,
        &snapshot,
        RestoreOptions {
            mode: RestoreMode::Merge,
            dry_run: true,
        },
    )
    .await
    .unwrap();
    assert!(dry.dry_run);
    assert_eq!(counts(&t.db).await, before, "a dry run wrote rows");
    assert!(dry.entries.inserted > 0 && dry.plays.inserted > 0);
    // The real run does exactly what the dry run said.
    let real = backup::restore(&t.db, eve.id, &snapshot, RestoreOptions::default())
        .await
        .unwrap();
    assert!(
        !RestoreOptions::default().dry_run,
        "the default keeps its work"
    );
    let mut said = dry.clone();
    said.dry_run = false;
    assert_eq!(said, real);
}

#[tokio::test]
async fn a_failing_restore_keeps_nothing() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    fixed_dataset(&t.db, ada.id).await;
    let snapshot = bytes(&export(&t.db, ada.id).await);
    let eve = common::user(&t.db).await;
    let before = counts(&t.db).await;
    // Every event insert fails, partway through the restore.
    common::break_event_log(&t.db).await;
    let failed = backup::restore(&t.db, eve.id, &snapshot, RestoreOptions::default()).await;
    assert!(matches!(failed, Err(RestoreError::Db(_))));
    common::repair_event_log(&t.db).await;
    assert_eq!(counts(&t.db).await, before, "a failed restore left rows");
}

#[tokio::test]
async fn secrets_leave_the_backup_unless_asked_for_and_a_channel_without_them_is_skipped() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    fixed_dataset(&t.db, ada.id).await;
    let redacted = backup::export_backup(&t.db, ada.id, ExportOptions::default())
        .await
        .unwrap();
    assert!(!redacted.secrets_included);
    let text = String::from_utf8(bytes(&redacted)).unwrap();
    assert!(!text.contains("s3cret") && !text.contains("hooks.example.com"));
    let [channel] = redacted.notification_channels.as_slice() else {
        panic!("one channel");
    };
    assert_eq!(channel.config, None);
    assert_eq!(channel.subscriptions.len(), 1, "subscriptions are kept");

    let eve = common::user(&t.db).await;
    let report = backup::restore(&t.db, eve.id, &bytes(&redacted), RestoreOptions::default())
        .await
        .unwrap();
    assert_eq!(report.notification_channels.skipped, 1);
    assert_eq!(report.notification_subscriptions.skipped, 1);
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("hook"));

    let full = export(&t.db, ada.id).await;
    assert!(String::from_utf8(bytes(&full)).unwrap().contains("s3cret"));
    let zoe = common::user(&t.db).await;
    let report = backup::restore(&t.db, zoe.id, &bytes(&full), RestoreOptions::default())
        .await
        .unwrap();
    assert_eq!(report.notification_channels.inserted, 1);
    assert_eq!(report.notification_subscriptions.inserted, 1);
}

#[tokio::test]
async fn a_v1_fixture_restores_into_an_empty_server() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let report = backup::restore(
        &t.db,
        ada.id,
        &fixture("backup-v1.json"),
        RestoreOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(report.read_version, 1);
    assert_eq!(report.titles.created, 2);
    assert_eq!(report.entries.inserted, 2);
    assert_eq!(report.plays.inserted, 3);
    assert_eq!(report.lists.inserted, 1);
    // The titles wait for the refresh job, carrying their names meanwhile.
    let one_piece = stacks_core::media::catalog::find_by_ref(
        &t.db,
        &stacks_core::media::MediaId::parse("anilist:show:21").unwrap(),
    )
    .await
    .unwrap()
    .expect("the title is in the catalog");
    assert_eq!(one_piece.title, "One Piece");
    assert_eq!(one_piece.fetched_at, None);
    assert!(
        stacks_core::media::catalog::find_by_external(
            &t.db,
            stacks_core::entity::enums::RefProvider::Mal,
            "21"
        )
        .await
        .unwrap()
        .is_some(),
        "its MAL id is linked too"
    );
    let library = LibraryService::new(t.db.clone());
    let view = library.get(ada.id, one_piece.id).await.unwrap().unwrap();
    let p = view.progress.unwrap();
    assert_eq!(
        (p.play_count, p.run_count),
        (2, 1),
        "the voided play does not count"
    );
    library
        .history(ada.id, None, None, 100)
        .await
        .expect("the fixture's history decodes");
}

#[tokio::test]
async fn a_v0_fixture_is_upgraded_and_restored() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let report = backup::restore(
        &t.db,
        ada.id,
        &fixture("backup-v0.json"),
        RestoreOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(report.read_version, 0);
    assert_eq!(report.entries.inserted, 2);
    let library = LibraryService::new(t.db.clone());
    let all = library.all(ada.id).await.unwrap();
    assert_eq!(all.len(), 2);
    let fight_club = all.iter().find(|v| v.entry.score == Some(90)).unwrap();
    assert_eq!(fight_club.entry.status, EntryStatus::Completed);
    let runs = library.runs(ada.id, fight_club.media.id).await.unwrap();
    assert_eq!(runs[0].completed_on, NaiveDate::from_ymd_opt(2024, 2, 11));
}

#[tokio::test]
async fn a_future_version_and_a_contradictory_backup_are_refused_before_any_write() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let before = counts(&t.db).await;
    let future = backup::restore(
        &t.db,
        ada.id,
        &fixture("backup-future.json"),
        RestoreOptions::default(),
    )
    .await;
    assert!(matches!(
        future,
        Err(RestoreError::Read(ReadError::UnsupportedVersion {
            found: 99,
            supported: 1
        }))
    ));

    let mut doc: Value = serde_json::from_slice(&fixture("backup-v1.json")).unwrap();
    doc["entries"][0]["title_id"] = json!(Uuid::now_v7());
    let dangling = backup::restore(
        &t.db,
        ada.id,
        doc.to_string().as_bytes(),
        RestoreOptions::default(),
    )
    .await;
    assert!(matches!(dangling, Err(RestoreError::Invalid(_))));

    let mut doc: Value = serde_json::from_slice(&fixture("backup-v1.json")).unwrap();
    doc["settings"]["score_format"] = json!("point_7");
    assert!(matches!(
        backup::restore(
            &t.db,
            ada.id,
            doc.to_string().as_bytes(),
            RestoreOptions::default()
        )
        .await,
        Err(RestoreError::Invalid(_))
    ));
    assert_eq!(counts(&t.db).await, before);
}
