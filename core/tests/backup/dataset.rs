//! A generated dataset for the backup tests: operations a user performs
//! through the services, and a proptest strategy over them. Split out of
//! `backup.rs` to keep that file within its length limit.

use chrono::NaiveDate;
use proptest::prelude::*;
use serde_json::json;
use stacks_core::Patch;
use stacks_core::blocklist::BlocklistService;
use stacks_core::entity::enums::{
    ChannelType, DatePrecision, EntryStatus, NotificationEventType, PlaySource,
};
use stacks_core::entity::media;
use stacks_core::library::{EntryPatch, LibraryService, PlayBatch, RunDate, RunPatch};
use stacks_core::lists::ListService;
use stacks_core::lists::service::CreateListRequest;
use stacks_core::notifications::NotificationService;
use stacks_core::notifications::service::NewChannel;
use stacks_core::tags::TagService;
use stacks_core::user::UserService;
use uuid::Uuid;

use super::common;

/// The titles a generated dataset draws on: three movies and three shows.
pub async fn titles(db: &sea_orm::DatabaseConnection) -> Vec<media::Model> {
    let mut out = Vec::new();
    for n in 0..3 {
        out.push(common::movie(db, 500 + n).await);
        out.push(common::show(db, 1300 + n).await);
    }
    out
}

#[derive(Debug, Clone)]
pub enum Op {
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
    /// Mark episodes 1 to `through` of `season` watched at once: one batch,
    /// whose events share a kind and a timestamp.
    PlayUpTo {
        title: usize,
        season: i32,
        through: i32,
    },
    Void(usize),
    /// Restore a voided play.
    Unvoid(usize),
    Delete(usize),
    Restore(usize),
    Rewatch(usize),
    Dates {
        title: usize,
        day: u32,
        precision: DatePrecision,
    },
    List(String),
    /// A smart list drawing from the library, naming the `tag`th tag when
    /// there is one.
    SmartList {
        tag: usize,
    },
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

pub fn op() -> impl Strategy<Value = Op> {
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
        2 => (title.clone(), 1i32..=2, 2i32..=4)
            .prop_map(|(title, season, through)| Op::PlayUpTo { title, season, through }),
        2 => any::<usize>().prop_map(Op::Void),
        1 => any::<usize>().prop_map(Op::Unvoid),
        1 => title.clone().prop_map(Op::Delete),
        1 => title.clone().prop_map(Op::Restore),
        1 => title.clone().prop_map(Op::Rewatch),
        1 => (title.clone(), 1u32..=28, precision())
            .prop_map(|(title, day, precision)| Op::Dates { title, day, precision }),
        1 => "[a-z]{1,8}".prop_map(Op::List),
        1 => (0usize..4).prop_map(|tag| Op::SmartList { tag }),
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
pub async fn apply(
    db: &sea_orm::DatabaseConnection,
    user: Uuid,
    titles: &[media::Model],
    ops: &[Op],
) {
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
            Op::PlayUpTo {
                title,
                season,
                through,
            } => {
                let episodes = (1..=*through).map(|e| (*season, e)).collect();
                let _ = library
                    .log_plays(
                        user,
                        &titles[*title],
                        PlayBatch::Episodes(episodes),
                        None,
                        PlaySource::Bulk,
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
            Op::Unvoid(i) => {
                let mut voided = Vec::new();
                for m in titles {
                    if let Ok(plays) = library.plays(user, m.id).await {
                        voided.extend(plays.into_iter().filter(|p| p.voided_at.is_some()));
                    }
                }
                if !voided.is_empty() {
                    let _ = library
                        .restore_play(user, voided[i % voided.len()].id)
                        .await;
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
            Op::SmartList { tag } => {
                let mut filter = json!({
                    "sort": "library",
                    "category": ["k-drama", "anime"],
                    "user_score_min": 80,
                });
                if let Some(t) = tag_ids.get(*tag) {
                    filter["tag"] = json!([t]);
                }
                let _ = lists
                    .create_list(
                        user,
                        CreateListRequest {
                            name: format!("smart {tag}"),
                            description: None,
                            is_smart: true,
                            filter_query: Some(filter),
                        },
                    )
                    .await;
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
