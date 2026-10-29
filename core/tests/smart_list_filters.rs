//! A smart list's stored filter through evaluation, export and restore:
//! rows an earlier restore stored unchecked, filters stored in a
//! non-canonical form, and the tags a restored filter may name.

mod common;

use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::{ActiveModelTrait, DatabaseConnection};
use serde_json::{Value, json};
use stacks_core::entity::enums::EntryStatus;
use stacks_core::entity::list;
use stacks_core::import_export::backup::{
    self, Backup, ExportOptions, RestoreError, RestoreMode, RestoreOptions, RestoreReport,
};
use stacks_core::library::{EntryPatch, LibraryService};
use stacks_core::lists::service::{CreateListRequest, ListService};
use stacks_core::media::filter::FilterQuery;
use stacks_core::media::{CameoProvider, ProviderSettings};
use stacks_core::tags::TagService;
use uuid::Uuid;

async fn smart_list(db: &DatabaseConnection, user: Uuid, filter: Value) -> list::Model {
    ListService::new(db.clone())
        .create_list(
            user,
            CreateListRequest {
                name: "Smart".into(),
                description: None,
                is_smart: true,
                filter_query: Some(filter),
            },
        )
        .await
        .expect("a smart list")
}

/// Write `filter` into the list's row as it is, as an earlier restore (or
/// API) could have.
async fn store_raw(db: &DatabaseConnection, list: &list::Model, filter: Value) {
    list::ActiveModel {
        id: Unchanged(list.id),
        filter_query: Set(Some(filter)),
        ..Default::default()
    }
    .update(db)
    .await
    .expect("stored");
}

async fn export(db: &DatabaseConnection, user: Uuid) -> Backup {
    let options = ExportOptions {
        include_secrets: false,
    };
    backup::export_backup(db, user, options)
        .await
        .expect("export")
}

async fn restore(
    db: &DatabaseConnection,
    user: Uuid,
    doc: &Backup,
    mode: RestoreMode,
) -> Result<RestoreReport, RestoreError> {
    let bytes = serde_json::to_vec(doc).expect("a backup serializes");
    let options = RestoreOptions {
        mode,
        dry_run: false,
    };
    backup::restore(db, user, &bytes, options).await
}

/// A provider no library-sourced list ever calls.
fn unreachable_provider() -> CameoProvider {
    let settings = ProviderSettings {
        tmdb_base_url: Some("http://127.0.0.1:9".into()),
        anilist_base_url: Some("http://127.0.0.1:9".into()),
        ..ProviderSettings::default()
    };
    CameoProvider::new("0123456789abcdef0123456789abcdef", &settings).expect("adapter")
}

/// `tag` and `category` in the shape a version 1 document could hold them:
/// they never narrowed the list, so it still evaluates and exports, by the
/// members that read.
#[tokio::test]
async fn a_stored_filter_with_unreadable_members_evaluates_and_exports() {
    let t = test_db!();
    let db = &t.db;
    let ada = common::user(db).await;
    let library = LibraryService::new(db.clone());
    let (done, going) = (common::movie(db, 1).await, common::movie(db, 2).await);
    for (m, status) in [
        (&done, EntryStatus::Completed),
        (&going, EntryStatus::Watching),
    ] {
        let patch = EntryPatch {
            status: Some(status),
            ..EntryPatch::default()
        };
        library.upsert(ada.id, m.id, patch).await.unwrap();
    }
    let list = smart_list(db, ada.id, json!({ "sort": "library" })).await;
    store_raw(
        db,
        &list,
        json!({
            "sort": "library", "status": ["completed"],
            "tag": ["comfort"], "category": "anime",
        }),
    )
    .await;

    let listing = ListService::new(db.clone())
        .evaluate_smart_list(&unreachable_provider(), ada.id, list.id)
        .await
        .expect("the list evaluates");
    let ids: Vec<&str> = listing
        .items
        .iter()
        .map(|m| m.cameo_media_id.as_str())
        .collect();
    assert_eq!(ids, ["tmdb:movie:1"]);

    let doc = export(db, ada.id).await;
    assert_eq!(
        doc.lists[0].filter,
        Some(FilterQuery {
            status: Some(vec!["completed".into()]),
            sort: Some("library".into()),
            ..FilterQuery::default()
        })
    );
}

/// A filter stored with `null` members, as an earlier API wrote them, means
/// what its canonical form means: restoring the account's own backup leaves
/// the list unchanged, and a filter that differs updates it.
#[tokio::test]
async fn a_same_account_restore_compares_filters_in_canonical_form() {
    let t = test_db!();
    let db = &t.db;
    let ada = common::user(db).await;
    let list = smart_list(db, ada.id, json!({ "status": ["completed"] })).await;
    store_raw(
        db,
        &list,
        json!({ "status": ["completed"], "genre": null, "year_min": null }),
    )
    .await;
    let mut doc = export(db, ada.id).await;

    for mode in [RestoreMode::Merge, RestoreMode::Overwrite] {
        let report = restore(db, ada.id, &doc, mode).await.unwrap();
        assert_eq!(
            (report.lists.unchanged, report.lists.updated),
            (1, 0),
            "{mode:?}"
        );
    }

    let filter = doc.lists[0].filter.as_mut().expect("a filter");
    filter.status = Some(vec!["watching".into()]);
    let report = restore(db, ada.id, &doc, RestoreMode::Overwrite)
        .await
        .unwrap();
    assert_eq!((report.lists.unchanged, report.lists.updated), (0, 1));
    let stored = ListService::new(db.clone())
        .get_list(ada.id, list.id)
        .await
        .unwrap();
    assert_eq!(stored.filter_query, Some(json!({ "status": ["watching"] })));
}

/// A tag the document lacks is kept as a deleted one, unless it is another
/// account's: that is refused, as creating the list would be.
#[tokio::test]
async fn a_restored_filter_never_names_another_accounts_tag() {
    let t = test_db!();
    let db = &t.db;
    let (ada, bob) = (common::user(db).await, common::user(db).await);
    let bobs = TagService::new(db.clone())
        .create_tag(bob.id, "bobs".into(), None)
        .await
        .unwrap();
    smart_list(db, ada.id, json!({ "sort": "library" })).await;
    let mut doc = export(db, ada.id).await;
    let mut naming = |tag: Uuid| {
        doc.lists[0].filter = Some(FilterQuery {
            tag: Some(vec![tag]),
            ..FilterQuery::default()
        });
        doc.clone()
    };

    let planted = naming(bobs.id);
    let refused = restore(db, ada.id, &planted, RestoreMode::Overwrite).await;
    assert!(
        matches!(&refused, Err(RestoreError::Invalid(why)) if why.contains("not yours")),
        "{refused:?}"
    );

    let deleted = Uuid::now_v7();
    let kept = naming(deleted);
    restore(db, ada.id, &kept, RestoreMode::Overwrite)
        .await
        .expect("a deleted tag stays");
    let stored = ListService::new(db.clone())
        .get_user_lists(ada.id)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(stored.filter_query, Some(json!({ "tag": [deleted] })));
}
