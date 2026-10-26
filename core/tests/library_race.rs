//! `LibraryService::replace` losing a concurrent create, forced
//! deterministically against PostgreSQL 18: a second connection holds an
//! uncommitted insert of the same entry, so the service's own insert blocks
//! on the unique index and then fails with a unique violation once that
//! transaction commits. A create lost that way did not make the entry live.

mod common;

use std::time::Duration;

use sea_orm::{
    ActiveModelTrait, ActiveValue::NotSet, ConnectionTrait, DatabaseConnection, EntityTrait, Set,
    Statement, TransactionTrait,
};
use stacks_core::entity::enums::EntryStatus;
use stacks_core::entity::{entry_event, library_entry};
use stacks_core::library::{EntryState, LibraryError, LibraryService};
use uuid::Uuid;

fn mine() -> EntryState {
    EntryState {
        status: EntryStatus::Planning,
        score: None,
        favourite: false,
        notes: Some("loser".into()),
    }
}

/// Wait (bounded) until some backend is blocked on a lock, which is the
/// service's insert waiting for the uncommitted winner.
async fn until_blocked(db: &DatabaseConnection) {
    for _ in 0..400 {
        let row = db
            .query_one_raw(Statement::from_string(
                db.get_database_backend(),
                "SELECT count(*) AS n FROM pg_stat_activity \
                 WHERE datname = current_database() AND wait_event_type = 'Lock'",
            ))
            .await
            .unwrap()
            .unwrap();
        if row.try_get::<i64>("", "n").unwrap() > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the service's insert never blocked on the concurrent create");
}

/// Run `replace` against a create that another transaction commits while the
/// service's insert waits on it. The winner's entry is returned with the
/// service's outcome.
async fn race(
    only_if_absent: bool,
) -> Option<(
    Result<(library_entry::Model, bool), LibraryError>,
    library_entry::Model,
    DatabaseConnection,
    common::TestDb,
)> {
    let t = common::TestDb::new().await?;
    let user = common::user(&t.db).await;
    let show = common::show(&t.db, 1).await;
    let other = t.second_pool().await;

    let winner = other.begin().await.unwrap();
    let won = library_entry::ActiveModel {
        id: Set(Uuid::now_v7()),
        user_id: Set(user.id),
        media_id: Set(show.id),
        status: Set(EntryStatus::Watching),
        score: Set(Some(70)),
        favourite: Set(true),
        notes: Set(Some("winner".into())),
        deleted_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(&winner)
    .await
    .unwrap();

    let svc = LibraryService::new(t.db.clone());
    let (user_id, media_id) = (user.id, show.id);
    let loser =
        tokio::spawn(async move { svc.replace(user_id, media_id, mine(), only_if_absent).await });
    until_blocked(&other).await;
    winner.commit().await.unwrap();
    let outcome = loser.await.unwrap();
    Some((outcome, won, other, t))
}

#[tokio::test]
async fn create_only_replace_that_loses_a_create_race_is_refused() {
    let Some((outcome, won, _other, t)) = race(true).await else {
        return;
    };
    assert!(
        matches!(outcome, Err(LibraryError::AlreadyLive)),
        "{outcome:?}"
    );
    let after = library_entry::Entity::find_by_id(won.id)
        .one(&t.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after, won, "the winner's entry is untouched");
    assert!(
        entry_event::Entity::find()
            .all(&t.db)
            .await
            .unwrap()
            .is_empty(),
        "the refused create wrote no event"
    );
}

#[tokio::test]
async fn replace_that_loses_a_create_race_replaces_but_did_not_create() {
    let Some((outcome, won, _other, t)) = race(false).await else {
        return;
    };
    let (entry, made_live) = outcome.unwrap();
    assert!(!made_live, "a lost create did not make the entry live");
    assert_eq!(entry.id, won.id);
    assert_eq!(entry.status, EntryStatus::Planning);
    assert_eq!(entry.notes.as_deref(), Some("loser"));
    assert_eq!(entry.score, None);
    assert!(!entry.favourite);
    let creates = entry_event::Entity::find()
        .all(&t.db)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == stacks_core::entity::enums::EntryEventKind::EntryCreated)
        .count();
    assert_eq!(creates, 0, "the loser recorded no creation");
}
