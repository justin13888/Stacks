//! Lists, tags and the blocklist against PostgreSQL 18.

mod common;

use stacks_core::Patch;
use stacks_core::blocklist::BlocklistService;
use stacks_core::entity::enums::EntryStatus;
use stacks_core::library::{EntryPatch, LibraryService};
use stacks_core::lists::ListError;
use stacks_core::lists::service::{CreateListRequest, ListService, MoveItem, UpdateListRequest};
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
    svc.move_item(
        user.id,
        list.id,
        ids[3],
        MoveItem {
            after: Some(ids[0]),
            before: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(order().await, vec![ids[0], ids[3], ids[1], ids[2]]);
    for _ in 0..60 {
        svc.move_item(
            user.id,
            list.id,
            ids[2],
            MoveItem {
                after: None,
                before: Some(ids[3]),
            },
        )
        .await
        .unwrap();
        svc.move_item(user.id, list.id, ids[2], MoveItem::default())
            .await
            .unwrap();
    }
    assert_eq!(order().await, vec![ids[0], ids[3], ids[1], ids[2]]);

    // A whole-list reorder passes through duplicate keys mid-transaction.
    svc.reorder(user.id, list.id, &[ids[2], ids[1], ids[0], ids[3]])
        .await
        .unwrap();
    assert_eq!(order().await, vec![ids[2], ids[1], ids[0], ids[3]]);

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
