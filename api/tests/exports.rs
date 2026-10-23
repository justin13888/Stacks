//! Exporting everything as a lossless JSON backup, and the library as CSV.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use kynos::http::StatusCode;
use serde_json::json;
use stacks_core::Patch;
use stacks_core::entity::enums::{ChannelType, DatePrecision, EntryStatus, Role};
use stacks_core::entity::media;
use stacks_core::import_export::backup::{Backup, FORMAT, FORMAT_VERSION};
use stacks_core::library::{EntryPatch, LibraryService, LogPlay, RunDate, RunPatch};
use stacks_core::media::MediaService;
use stacks_core::notifications::NotificationService;
use stacks_core::notifications::service::NewChannel;
use stacks_core::tags::TagService;
use support::metadata::{self, BREAKING_BAD, FIGHT_CLUB, ONE_PIECE};
use support::{Credentials, Env, TestUser};

/// Track a title through core, as the library area would.
async fn track(env: &Env, user: &TestUser, raw: &str, patch: EntryPatch) -> media::Model {
    let media = MediaService::new(env.db.db.clone(), Arc::new(env.metadata.clone()))
        .ensure_in_catalog(&metadata::id(raw))
        .await
        .unwrap();
    LibraryService::new(env.db.db.clone())
        .upsert(user.id(), media.id, patch)
        .await
        .unwrap();
    media
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[tokio::test]
async fn the_backup_holds_everything_and_the_csv_the_live_library() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let eve = env.user(Role::User).await;
    let library = LibraryService::new(env.db.db.clone());

    let fight_club = track(
        &env,
        &ada,
        FIGHT_CLUB,
        EntryPatch {
            status: Some(EntryStatus::Completed),
            score: Patch::Value(90),
            notes: Patch::Value("Rule one, rule two".into()),
            ..Default::default()
        },
    )
    .await;
    let started = chrono::NaiveDate::from_ymd_opt(2024, 3, 1).unwrap();
    library
        .update_run_by_number(
            ada.id(),
            fight_club.id,
            1,
            RunPatch {
                started: Patch::Value(RunDate {
                    date: started,
                    precision: DatePrecision::Day,
                }),
                completed: Patch::Absent,
            },
        )
        .await
        .unwrap();
    let breaking_bad = track(&env, &ada, BREAKING_BAD, EntryPatch::default()).await;
    let play = library
        .log_play(ada.id(), &breaking_bad, LogPlay::manual(Some((1, 1))))
        .await
        .unwrap();
    library
        .log_play(ada.id(), &breaking_bad, LogPlay::manual(Some((1, 2))))
        .await
        .unwrap();
    library.void_play(ada.id(), play.id).await.unwrap();
    let tag = TagService::new(env.db.db.clone())
        .create_tag(ada.id(), "crime".into(), None)
        .await
        .unwrap();
    TagService::new(env.db.db.clone())
        .assign_tag(ada.id(), breaking_bad.id, tag.id)
        .await
        .unwrap();
    NotificationService::new(env.db.db.clone())
        .create_channel(
            ada.id(),
            NewChannel {
                channel_type: ChannelType::Webhook,
                name: "hook".into(),
                enabled: true,
                config: json!({"url": "https://hooks.example.com/abc", "secret": "s3cret"}),
            },
        )
        .await
        .unwrap();
    track(&env, &eve, ONE_PIECE, EntryPatch::default()).await;
    // A trashed entry is still the caller's, so it is in the backup.
    let one_piece = track(&env, &ada, ONE_PIECE, EntryPatch::default()).await;
    library.delete(ada.id(), one_piece.id).await.unwrap();

    let response = client
        .get("/api/v1/exports/backup")
        .bearer(&ada.read)
        .send()
        .await;
    response.assert_status(StatusCode::OK);
    assert_eq!(response.header("content-type"), Some("application/json"));
    let disposition = response.header("content-disposition").expect("a filename");
    assert!(disposition.starts_with("attachment"), "{disposition}");
    assert!(
        disposition.contains(&format!("stacks-backup-{}.json", today())),
        "{disposition}"
    );
    let backup: Backup = serde_json::from_slice(response.bytes()).expect("a backup document");
    assert_eq!(backup.format, FORMAT);
    assert_eq!(backup.format_version, FORMAT_VERSION);
    assert_eq!(backup.user.as_ref().map(|u| u.id), Some(ada.id()));
    assert_eq!(backup.entries.len(), 3, "live and trashed entries");
    let entry = |m: &media::Model| {
        backup
            .entries
            .iter()
            .find(|e| e.title_id == m.id)
            .expect("the entry is in the backup")
    };
    let movie = entry(&fight_club);
    assert_eq!(movie.status, EntryStatus::Completed);
    assert_eq!(movie.score, Some(90));
    assert_eq!(movie.notes.as_deref(), Some("Rule one, rule two"));
    assert_eq!(movie.runs[0].started_on, Some(started));
    let show = entry(&breaking_bad);
    let plays = &show.runs[0].plays;
    assert_eq!(plays.len(), 2, "the voided play is kept");
    assert!(plays.iter().any(|p| p.voided_at.is_some()));
    assert_eq!(show.tags.len(), 1);
    assert!(show.events.len() >= 4, "the event log is in the backup");
    assert!(entry(&one_piece).deleted_at.is_some());
    let title = backup.titles.iter().find(|t| t.id == one_piece.id).unwrap();
    assert!(
        title
            .refs
            .iter()
            .any(|r| r.primary && r.external_id == "21")
    );
    // Without asking, the channel's credentials stay out.
    assert_eq!(backup.notification_channels[0].config, None);
    assert!(!response.text().contains("s3cret"));
    let with_secrets: Backup = serde_json::from_slice(
        client
            .get("/api/v1/exports/backup?include_secrets=true")
            .session(&ada)
            .send()
            .await
            .assert_status(StatusCode::OK)
            .bytes(),
    )
    .unwrap();
    assert!(with_secrets.secrets_included);
    assert_eq!(
        with_secrets.notification_channels[0]
            .config
            .as_ref()
            .and_then(|c| c.get("secret")),
        Some(&json!("s3cret"))
    );
    // A token never reads a credential back, whatever scopes it holds.
    for token in [&ada.read, &ada.read_write] {
        let refused = client
            .get("/api/v1/exports/backup?include_secrets=true")
            .bearer(token)
            .send()
            .await;
        refused
            .assert_status(StatusCode::FORBIDDEN)
            .assert_problem_type(&support::problem("secrets-need-session"));
        assert!(!refused.text().contains("s3cret"));
    }
    client
        .get("/api/v1/exports/backup?include_secrets=false")
        .bearer(&ada.read)
        .send()
        .await
        .assert_status(StatusCode::OK);
    client
        .get("/api/v1/exports/backup?include_secrets=perhaps")
        .session(&ada)
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    let csv = client.get("/api/v1/exports/csv").session(&ada).send().await;
    csv.assert_status(StatusCode::OK);
    assert_eq!(csv.header("content-type"), Some("text/csv; charset=utf-8"));
    assert!(
        csv.header("content-disposition")
            .unwrap()
            .contains(&format!("stacks-library-{}.csv", today()))
    );
    let mut reader = csv::Reader::from_reader(csv.bytes().as_ref());
    let header: Vec<String> = reader
        .headers()
        .unwrap()
        .iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(header, stacks_core::import_export::export::csv::HEADER);
    let rows: Vec<csv::StringRecord> = reader.records().map(Result::unwrap).collect();
    assert_eq!(rows.len(), 2, "only live entries");
    let row = |r: &str| rows.iter().find(|x| &x[0] == r).unwrap().clone();
    let movie = row(FIGHT_CLUB);
    assert_eq!(
        (&movie[1], &movie[4], &movie[5], &movie[11]),
        ("Fight Club", "completed", "90", "2024-03-01")
    );
    let show = row(BREAKING_BAD);
    // Episode 2 is watched; episode 1's play was undone.
    assert_eq!(
        (&show[4], &show[7], &show[9], &show[10]),
        ("watching", "1", "1", "0")
    );
    assert_eq!(&show[15], "crime");

    // An empty library is an empty export, not an error.
    let empty: Backup = serde_json::from_slice(
        client
            .get("/api/v1/exports/backup")
            .session(&env.user(Role::User).await)
            .send()
            .await
            .assert_status(StatusCode::OK)
            .bytes(),
    )
    .unwrap();
    assert!(empty.entries.is_empty() && empty.titles.is_empty());

    client.assert_conformance();
    support::sweep::sweep(&env, "exports").await;
    env.assert_covered("exports");
}
