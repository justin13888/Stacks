//! Lists, tags and the blocklist against PostgreSQL 18.

mod common;

use stacks_core::Patch;
use stacks_core::blocklist::BlocklistService;
use stacks_core::entity::enums::EntryStatus;
use stacks_core::library::{EntryPatch, LibraryService};
use stacks_core::lists::ListError;
use stacks_core::lists::service::{CreateListRequest, ListService, Placement, UpdateListRequest};
use stacks_core::tags::{TagError, TagService};

fn manual(name: &str) -> CreateListRequest {
    CreateListRequest {
        name: name.into(),
        description: Some("desc".into()),
        is_smart: false,
        filter_query: None,
    }
}

#[tokio::test]
async fn lists_soft_delete_and_restore_with_their_items() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let m = common::movie(db, 1).await;
    let svc = ListService::new(db.clone());
    let list = svc
        .create_list(user.id, manual("Favourites"))
        .await
        .unwrap();
    svc.add_item(user.id, list.id, m.id, None).await.unwrap();

    svc.delete_list(user.id, list.id).await.unwrap();
    assert!(svc.get_user_lists(user.id).await.unwrap().is_empty());
    assert!(matches!(
        svc.get_list(user.id, list.id).await,
        Err(ListError::NotFound)
    ));
    assert_eq!(svc.trash(user.id).await.unwrap().len(), 1);

    let restored = svc.restore_list(user.id, list.id).await.unwrap();
    assert!(restored.deleted_at.is_none());
    assert_eq!(svc.item_count(list.id).await.unwrap(), 1);
    let media = svc.list_media(user.id, list.id).await.unwrap();
    assert_eq!(media.len(), 1);
    assert_eq!(media[0].cameo_media_id, "tmdb:movie:1");
}

#[tokio::test]
async fn list_update_is_a_merge_patch() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = ListService::new(db.clone());
    let list = svc.create_list(user.id, manual("A")).await.unwrap();
    let renamed = svc
        .update_list(
            user.id,
            list.id,
            UpdateListRequest {
                name: Some("B".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(renamed.name, "B");
    assert_eq!(renamed.description.as_deref(), Some("desc"));
    let cleared = svc
        .update_list(
            user.id,
            list.id,
            UpdateListRequest {
                description: Patch::Null,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cleared.description, None);
    assert_eq!(cleared.name, "B");
}

#[tokio::test]
async fn list_order_uses_string_positions() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = ListService::new(db.clone());
    let list = svc.create_list(user.id, manual("Order")).await.unwrap();
    let mut ids = Vec::new();
    for n in 1..=4 {
        let m = common::movie(db, 100 + n).await;
        svc.add_item(user.id, list.id, m.id, None).await.unwrap();
        ids.push(m.id);
    }
    let order = || async {
        stacks_core::lists::repo::items_for_list(db, list.id)
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.media_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(order().await, ids);

    // Move the last item between the first two, many times over.
    svc.move_item(user.id, list.id, ids[3], Placement::after(ids[0]))
        .await
        .unwrap();
    assert_eq!(order().await, vec![ids[0], ids[3], ids[1], ids[2]]);
    for _ in 0..60 {
        svc.move_item(user.id, list.id, ids[2], Placement::before(ids[3]))
            .await
            .unwrap();
        svc.move_item(user.id, list.id, ids[2], Placement::default())
            .await
            .unwrap();
    }
    assert_eq!(order().await, vec![ids[0], ids[3], ids[1], ids[2]]);

    assert!(matches!(
        svc.add_item(user.id, list.id, ids[0], None).await,
        Err(ListError::AlreadyInList)
    ));
}

#[tokio::test]
async fn lists_are_private() {
    let t = test_db!();
    let db = &t.db;
    let owner = common::user(db).await;
    let other = common::user(db).await;
    let svc = ListService::new(db.clone());
    let list = svc.create_list(owner.id, manual("Mine")).await.unwrap();
    assert!(matches!(
        svc.get_list(other.id, list.id).await,
        Err(ListError::NotOwner)
    ));
    assert!(svc.delete_list(other.id, list.id).await.is_err());
}

#[tokio::test]
async fn tags_attach_to_library_entries() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let m = common::movie(db, 1).await;
    let tags = TagService::new(db.clone());
    let a = tags.create_tag(user.id, "a".into(), None).await.unwrap();
    let b = tags
        .create_tag(user.id, "b".into(), Some("#ff0000".into()))
        .await
        .unwrap();
    assert!(matches!(
        tags.create_tag(user.id, "a".into(), None).await,
        Err(TagError::NameTaken)
    ));

    // A title must be in the library to be tagged.
    assert!(matches!(
        tags.assign_tag(user.id, m.id, a.id).await,
        Err(TagError::NotTracked)
    ));
    LibraryService::new(db.clone())
        .upsert(
            user.id,
            m.id,
            EntryPatch {
                status: Some(EntryStatus::Planning),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let set = tags
        .set_tags_for_media(user.id, m.id, vec![a.id, b.id])
        .await
        .unwrap();
    assert_eq!(set.len(), 2);
    tags.assign_tag(user.id, m.id, a.id).await.unwrap(); // idempotent
    assert_eq!(
        tags.get_media_for_tag(user.id, a.id).await.unwrap(),
        vec![m.id]
    );

    // Deleting a tag unassigns it; the entry stays.
    tags.delete_tag(user.id, a.id).await.unwrap();
    let left = tags.get_tags_for_media(user.id, m.id).await.unwrap();
    assert_eq!(left.iter().map(|t| t.id).collect::<Vec<_>>(), vec![b.id]);

    // Clearing a colour is an explicit null; a rename keeps the colour.
    let renamed = tags
        .update_tag(user.id, b.id, Some("bee".into()), Patch::Absent)
        .await
        .unwrap();
    assert_eq!(renamed.color.as_deref(), Some("#ff0000"));
    let cleared = tags
        .update_tag(user.id, b.id, None, Patch::Null)
        .await
        .unwrap();
    assert_eq!(cleared.color, None);
    assert_eq!(cleared.name, "bee");
}

#[tokio::test]
async fn blocklist_is_idempotent_and_filters_refs() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let m = common::movie(db, 1).await;
    let svc = BlocklistService::new(db.clone());
    let a = svc.block(user.id, m.id).await.unwrap();
    let b = svc.block(user.id, m.id).await.unwrap();
    assert_eq!(a.id, b.id);
    assert!(svc.is_blocked(user.id, m.id).await.unwrap());
    let kept = svc
        .filter_blocked(user.id, vec!["tmdb:movie:1".into(), "tmdb:movie:2".into()])
        .await
        .unwrap();
    assert_eq!(kept, vec!["tmdb:movie:2".to_string()]);
    svc.unblock(user.id, m.id).await.unwrap();
    assert!(!svc.is_blocked(user.id, m.id).await.unwrap());
}

#[tokio::test]
async fn items_are_placed_where_asked_and_keep_their_notes() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = ListService::new(db.clone());
    let list = svc.create_list(user.id, manual("Placed")).await.unwrap();
    let a = common::movie(db, 1).await.id;
    let b = common::movie(db, 2).await.id;
    let c = common::movie(db, 3).await.id;
    svc.add_item(user.id, list.id, a, Some("first".into()))
        .await
        .unwrap();
    // Before the first, then between the two.
    svc.add_item_at(user.id, list.id, b, None, Placement::before(a))
        .await
        .unwrap();
    let both = Placement {
        after: Patch::Value(b),
        before: Patch::Value(a),
    };
    svc.add_item_at(user.id, list.id, c, None, both)
        .await
        .unwrap();
    let order = || async {
        stacks_core::lists::repo::items_for_list(db, list.id)
            .await
            .unwrap()
            .into_iter()
            .map(|i| (i.media_id, i.notes))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        order().await,
        vec![(b, None), (c, None), (a, Some("first".into()))]
    );

    // Neighbours that are not neighbours (without `c`, the list is b, a),
    // the item itself, a stranger.
    let apart = Placement {
        after: Patch::Value(a),
        before: Patch::Value(b),
    };
    assert!(matches!(
        svc.move_item(user.id, list.id, c, apart).await,
        Err(ListError::InvalidPlacement(_))
    ));
    assert!(matches!(
        svc.move_item(user.id, list.id, c, Placement::after(c))
            .await,
        Err(ListError::InvalidPlacement(_))
    ));
    let stranger = common::movie(db, 4).await.id;
    assert!(matches!(
        svc.move_item(user.id, list.id, c, Placement::after(stranger))
            .await,
        Err(ListError::AnchorNotFound)
    ));
    assert!(matches!(
        svc.move_item(user.id, list.id, stranger, Placement::default())
            .await,
        Err(ListError::ItemNotFound)
    ));

    // To the start; notes merge without moving.
    let start = Placement {
        after: Patch::Null,
        before: Patch::Absent,
    };
    svc.move_item(user.id, list.id, a, start).await.unwrap();
    let noted = svc
        .update_item(user.id, list.id, a, None, Patch::Value("again".into()))
        .await
        .unwrap();
    assert_eq!(noted.notes.as_deref(), Some("again"));
    let kept = svc
        .update_item(
            user.id,
            list.id,
            a,
            Some(Placement::after(c)),
            Patch::Absent,
        )
        .await
        .unwrap();
    assert_eq!(kept.notes.as_deref(), Some("again"));
    assert_eq!(
        order()
            .await
            .into_iter()
            .map(|(m, _)| m)
            .collect::<Vec<_>>(),
        vec![b, c, a]
    );
    let cleared = svc
        .update_item(user.id, list.id, a, None, Patch::Null)
        .await
        .unwrap();
    assert_eq!(cleared.notes, None);
    assert_eq!(cleared.position, kept.position);
}

#[tokio::test]
async fn smart_lists_take_a_checked_filter_and_no_items() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let svc = ListService::new(db.clone());
    let smart = svc
        .create_list(
            user.id,
            CreateListRequest {
                name: "  Smart  ".into(),
                description: None,
                is_smart: true,
                filter_query: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(smart.name, "Smart");
    assert_eq!(smart.filter_query, Some(serde_json::json!({})));
    let m = common::movie(db, 1).await;
    assert!(matches!(
        svc.add_item(user.id, smart.id, m.id, None).await,
        Err(ListError::NotManual)
    ));
    let bad = UpdateListRequest {
        filter_query: Patch::Value(serde_json::json!({"year_min": 2000, "year_max": 1990})),
        ..Default::default()
    };
    assert!(matches!(
        svc.update_list(user.id, smart.id, bad).await,
        Err(ListError::InvalidFilter(_))
    ));
    let cleared = svc
        .update_list(
            user.id,
            smart.id,
            UpdateListRequest {
                filter_query: Patch::Null,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cleared.filter_query, Some(serde_json::json!({})));

    // A manual list has no filter to set.
    let plain = svc.create_list(user.id, manual("Plain")).await.unwrap();
    let filtered = UpdateListRequest {
        filter_query: Patch::Value(serde_json::json!({})),
        ..Default::default()
    };
    assert!(matches!(
        svc.update_list(user.id, plain.id, filtered).await,
        Err(ListError::NotSmart)
    ));
    assert!(matches!(
        svc.create_list(
            user.id,
            CreateListRequest {
                filter_query: Some(serde_json::json!({})),
                ..manual("x")
            }
        )
        .await,
        Err(ListError::NotSmart)
    ));

    // Restoring a live list is a no-op; another user's is missing.
    let restored = svc.restore_list(user.id, plain.id).await.unwrap();
    assert!(restored.deleted_at.is_none());
    let other = common::user(db).await;
    assert!(matches!(
        svc.restore_list(other.id, plain.id).await,
        Err(ListError::NotFound)
    ));
}

#[tokio::test]
async fn tag_names_trim_colours_are_checked_and_counts_skip_the_trash() {
    let t = test_db!();
    let db = &t.db;
    let user = common::user(db).await;
    let other = common::user(db).await;
    let tags = TagService::new(db.clone());
    let a = tags.create_tag(user.id, " a ".into(), None).await.unwrap();
    assert_eq!(a.name, "a");
    assert!(matches!(
        tags.create_tag(user.id, "a".into(), None).await,
        Err(TagError::NameTaken)
    ));
    for bad in ["red", "#ff00", "#gg0000", "ff0000"] {
        assert!(matches!(
            tags.create_tag(user.id, "b".into(), Some(bad.into())).await,
            Err(TagError::InvalidColor)
        ));
        assert!(matches!(
            tags.update_tag(user.id, a.id, None, Patch::Value(bad.into()))
                .await,
            Err(TagError::InvalidColor)
        ));
    }
    assert!(matches!(
        tags.update_tag(user.id, a.id, Some("  ".into()), Patch::Absent)
            .await,
        Err(TagError::EmptyName)
    ));

    let library = LibraryService::new(db.clone());
    let one = common::movie(db, 1).await;
    let two = common::movie(db, 2).await;
    for m in [&one, &two] {
        library
            .upsert(user.id, m.id, EntryPatch::default())
            .await
            .unwrap();
        tags.assign_tag(user.id, m.id, a.id).await.unwrap();
    }
    let count = || async {
        tags.entry_counts(&[a.id])
            .await
            .unwrap()
            .get(&a.id)
            .copied()
    };
    assert_eq!(count().await, Some(2));
    library.delete(user.id, two.id).await.unwrap();
    assert_eq!(count().await, Some(1));

    // Untagging is idempotent, but only with one's own tag.
    tags.unassign_tag(user.id, one.id, a.id).await.unwrap();
    tags.unassign_tag(user.id, one.id, a.id).await.unwrap();
    assert!(matches!(
        tags.unassign_tag(other.id, one.id, a.id).await,
        Err(TagError::NotFound)
    ));
}

/// A smart list names only the caller's tags, keeps one deleted since, and
/// follows its tag into a second account a backup is restored into.
#[tokio::test]
async fn a_smart_list_names_its_owners_tags() {
    use stacks_core::import_export::backup::{self, ExportOptions, RestoreOptions};
    let t = test_db!();
    let db = &t.db;
    let (ada, bob) = (common::user(db).await, common::user(db).await);
    let tags = TagService::new(db.clone());
    let comfort = tags
        .create_tag(ada.id, "comfort".into(), None)
        .await
        .unwrap();
    let bobs = tags.create_tag(bob.id, "bobs".into(), None).await.unwrap();
    let svc = ListService::new(db.clone());
    let smart = |tag: uuid::Uuid| CreateListRequest {
        filter_query: Some(serde_json::json!({ "tag": [tag], "sort": "library" })),
        is_smart: true,
        ..manual("Comfort")
    };
    assert!(matches!(
        svc.create_list(ada.id, smart(bobs.id)).await,
        Err(ListError::InvalidFilter(_))
    ));
    let list = svc.create_list(ada.id, smart(comfort.id)).await.unwrap();

    // Deleted since, the tag may stay in the filter; it cannot be added.
    tags.delete_tag(ada.id, comfort.id).await.unwrap();
    let keep = |tags: serde_json::Value| UpdateListRequest {
        filter_query: Patch::Value(serde_json::json!({ "tag": tags })),
        ..Default::default()
    };
    svc.update_list(ada.id, list.id, keep(serde_json::json!([comfort.id])))
        .await
        .unwrap();
    assert!(matches!(
        svc.update_list(ada.id, list.id, keep(serde_json::json!([bobs.id])))
            .await,
        Err(ListError::InvalidFilter(_))
    ));

    // Restored into a second account, the tag gets a new id the filter
    // follows.
    let rest = tags.create_tag(ada.id, "rest".into(), None).await.unwrap();
    svc.update_list(ada.id, list.id, keep(serde_json::json!([rest.id])))
        .await
        .unwrap();
    let options = ExportOptions {
        include_secrets: false,
    };
    let doc = backup::export_backup(db, ada.id, options).await.unwrap();
    let eve = common::user(db).await;
    let bytes = serde_json::to_vec(&doc).unwrap();
    backup::restore(db, eve.id, &bytes, RestoreOptions::default())
        .await
        .unwrap();
    let eves_rest = tags.list_tags(eve.id).await.unwrap().remove(0);
    assert_ne!(eves_rest.id, rest.id);
    let restored = svc.get_user_lists(eve.id).await.unwrap().remove(0);
    assert_eq!(
        restored.filter_query,
        Some(serde_json::json!({ "tag": [eves_rest.id] }))
    );
}
