//! Users, the explicit purge, and the catalog snapshot.

mod common;

use stacks_core::entity::enums::{EntryStatus, MediaKind, Role};
use stacks_core::library::{EntryPatch, LibraryService, LogPlay};
use stacks_core::lists::service::{CreateListRequest, ListService};
use stacks_core::media::catalog;
use stacks_core::media::{MediaId, Provider};
use stacks_core::tags::TagService;
use stacks_core::user::{PurgeConfirmation, UserError, UserService, purge};

/// Give a user one of everything.
async fn populate(db: &sea_orm::DatabaseConnection, user_id: uuid::Uuid) {
    let show = common::show(db, 1).await;
    let library = LibraryService::new(db.clone());
    let p = library
        .log_play(user_id, &show, LogPlay::manual(Some((1, 1))))
        .await
        .unwrap();
    library.void_play(user_id, p.id).await.unwrap();
    let tags = TagService::new(db.clone());
    let tag = tags.create_tag(user_id, "t".into(), None).await.unwrap();
    tags.assign_tag(user_id, show.id, tag.id).await.unwrap();
    let lists = ListService::new(db.clone());
    let list = lists
        .create_list(
            user_id,
            CreateListRequest {
                name: "l".into(),
                description: None,
                is_smart: false,
                filter_query: None,
            },
        )
        .await
        .unwrap();
    lists
        .add_item(user_id, list.id, show.id, None)
        .await
        .unwrap();
    stacks_core::blocklist::BlocklistService::new(db.clone())
        .block(user_id, show.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn deleting_a_user_means_deactivating() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    populate(db, user.id).await;
    let svc = UserService::new(db.clone());
    let off = svc.deactivate(user.id).await.unwrap();
    assert!(!off.is_active());
    // Everything is still there.
    assert_eq!(common::count(db, "library_entries").await, 1);
    assert_eq!(common::count(db, "plays").await, 1);
    let on = svc.reactivate(user.id).await.unwrap();
    assert!(on.is_active());
}

#[tokio::test]
async fn purge_needs_both_confirmations_then_removes_everything() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let bystander = common::user(db).await;
    populate(db, user.id).await;
    let svc = UserService::new(db.clone());
    let confirm = PurgeConfirmation {
        user_id: user.id,
        display_name: user.display_name.clone(),
    };

    assert!(matches!(
        purge(db, &confirm).await,
        Err(UserError::PurgeRequiresDeactivation)
    ));
    svc.deactivate(user.id).await.unwrap();
    let wrong = PurgeConfirmation {
        display_name: "someone else".into(),
        ..confirm.clone()
    };
    assert!(matches!(
        purge(db, &wrong).await,
        Err(UserError::PurgeNotConfirmed)
    ));
    assert_eq!(common::count(db, "users").await, 2);

    purge(db, &confirm).await.unwrap();
    assert!(matches!(
        svc.get_by_id(user.id).await,
        Err(UserError::NotFound)
    ));
    svc.get_by_id(bystander.id).await.unwrap();
    for table in [
        "library_entries",
        "watch_runs",
        "plays",
        "entry_events",
        "lists",
        "list_items",
        "tags",
        "entry_tags",
        "blocklist",
    ] {
        assert_eq!(common::count(db, table).await, 0, "{table} not purged");
    }
    // The catalog is shared and stays.
    assert_eq!(common::count(db, "media").await, 1);
    // And the delete guards are back on outside the purge's transaction.
    use sea_orm::ConnectionTrait;
    assert!(db.execute_unprepared("DELETE FROM users").await.is_err());
}

#[tokio::test]
async fn settings_merge_and_validate() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = UserService::new(db.clone());
    let u = svc
        .update_settings(
            user.id,
            serde_json::json!({"theme": "dark", "future_key": 1}),
        )
        .await
        .unwrap();
    let u2 = svc
        .update_settings(user.id, serde_json::json!({"title_language": "romaji"}))
        .await
        .unwrap();
    assert_eq!(u2.settings["theme"], "dark", "untouched key kept");
    assert_eq!(u2.settings["future_key"], 1, "unknown key kept");
    assert_eq!(u2.settings["title_language"], "romaji");
    let u3 = svc
        .update_settings(user.id, serde_json::json!({"theme": null}))
        .await
        .unwrap();
    assert!(u3.settings.get("theme").is_none());
    assert!(matches!(
        svc.update_settings(user.id, serde_json::json!({"theme": "neon"}))
            .await,
        Err(UserError::InvalidSettings(_))
    ));
    assert!(matches!(
        svc.update_settings(user.id, serde_json::json!([])).await,
        Err(UserError::InvalidSettings(_))
    ));
    let _ = u;
    let admin = svc.update_role(user.id, Role::Admin).await.unwrap();
    assert!(admin.is_admin());
}

#[tokio::test]
async fn snapshots_upsert_in_place_and_resolve_by_ref() {
    let t = test_db!();
    let db = &t.db;
    let mut snap = common::snapshot(MediaKind::Show, 42, "Old title");
    snap.origin_countries = vec!["KR".into()];
    snap.extra_refs = vec![(
        stacks_core::entity::enums::RefProvider::Imdb,
        "tt0000042".into(),
    )];
    let first = catalog::upsert_snapshot(db, &snap).await.unwrap();
    assert_eq!(first.category, stacks_core::entity::enums::Category::KDrama);
    snap.title = "New title".into();
    let second = catalog::upsert_snapshot(db, &snap).await.unwrap();
    assert_eq!(first.id, second.id, "the UUID never changes");
    assert_eq!(second.title, "New title");
    assert_eq!(common::count(db, "media").await, 1);
    assert_eq!(common::count(db, "media_refs").await, 2);

    let id = MediaId::new(Provider::Tmdb, MediaKind::Show, 42);
    assert_eq!(
        catalog::find_by_ref(db, &id).await.unwrap().unwrap().id,
        first.id
    );
    let refs = catalog::primary_refs(db, &[first.id]).await.unwrap();
    assert_eq!(refs[&first.id], id);
    let by_imdb = catalog::find_by_external(
        db,
        stacks_core::entity::enums::RefProvider::Imdb,
        "tt0000042",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(by_imdb.id, first.id);
}

#[tokio::test]
async fn placeholder_snapshot_tracks_without_a_provider() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let id = MediaId::new(Provider::AniList, MediaKind::Show, 21);
    let placeholder = catalog::insert_placeholder(db, &id, "upstream timed out")
        .await
        .unwrap();
    assert_eq!(placeholder.refresh_failures, 1);
    assert_eq!(
        placeholder.category,
        stacks_core::entity::enums::Category::Anime
    );
    assert!(placeholder.fetched_at.is_none());
    // Idempotent.
    let again = catalog::insert_placeholder(db, &id, "again").await.unwrap();
    assert_eq!(again.id, placeholder.id);

    LibraryService::new(db.clone())
        .upsert(
            user.id,
            placeholder.id,
            EntryPatch {
                status: Some(EntryStatus::Watching),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let (rows, pages) = LibraryService::new(db.clone())
        .list(user.id, &Default::default(), 1, 50)
        .await
        .unwrap();
    assert_eq!((rows.len(), pages), (1, 1));
}

#[tokio::test]
async fn administering_an_account_changes_role_and_activation_but_not_ones_own() {
    let t = test_db!();
    let db = &t.db;
    let svc = UserService::new(db.clone());
    let root = svc
        .update_role(common::user(db).await.id, Role::Admin)
        .await
        .unwrap();
    let ada = common::user(db).await;

    let promoted = svc
        .administer(root.id, ada.id, Some(Role::Admin), None)
        .await
        .unwrap();
    assert_eq!(promoted.role, Role::Admin);
    let off = svc
        .administer(root.id, ada.id, Some(Role::User), Some(true))
        .await
        .unwrap();
    assert_eq!(off.role, Role::User);
    assert!(!off.is_active());
    // Deactivating again keeps the first timestamp.
    let again = svc
        .administer(root.id, ada.id, None, Some(true))
        .await
        .unwrap();
    assert_eq!(again.deactivated_at, off.deactivated_at);
    let on = svc
        .administer(root.id, ada.id, None, Some(false))
        .await
        .unwrap();
    assert!(on.is_active());

    // Not one's own demotion or deactivation; restating is fine.
    for (role, deactivated) in [(Some(Role::User), None), (None, Some(true))] {
        assert!(matches!(
            svc.administer(root.id, root.id, role, deactivated).await,
            Err(UserError::CannotChangeOwnAccount)
        ));
    }
    let same = svc
        .administer(root.id, root.id, Some(Role::Admin), Some(false))
        .await
        .unwrap();
    assert_eq!(same.role, Role::Admin);
    assert!(same.is_active());
    assert!(matches!(
        svc.administer(root.id, uuid::Uuid::now_v7(), None, Some(true))
            .await,
        Err(UserError::NotFound)
    ));

    // Pages are newest first and cover everyone once.
    let first = svc.page(None, 1).await.unwrap();
    assert_eq!(first[0].id, ada.id);
    let rest = svc.page(Some(ada.id), 10).await.unwrap();
    assert_eq!(rest.iter().map(|u| u.id).collect::<Vec<_>>(), [root.id]);
}
