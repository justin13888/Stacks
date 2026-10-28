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

/// Everything a subscriber writes, for reading the audit line back.
#[derive(Clone, Default)]
struct Log(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Log {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl std::io::Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Log {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Give a user the rows the services above don't: credentials, an import
/// with an item and its queued job, a notification channel with a
/// subscription, and a queued notification.
async fn credentials_and_ops(db: &sea_orm::DatabaseConnection, user_id: uuid::Uuid) {
    use sea_orm::ConnectionTrait;
    // Unique per user, and the 64 hex digits both hash columns require.
    let hash = user_id.simple().to_string().repeat(2);
    db.execute_unprepared(&format!(
        "INSERT INTO sessions (user_id, token_hash, expires_at) \
         VALUES ('{user_id}', '{hash}', now() + interval '1 day');
         INSERT INTO api_tokens (user_id, name, token_prefix, lookup_hash, scopes) \
         VALUES ('{user_id}', 'cli', 'stk_abc', '{hash}', ARRAY['read']);
         WITH i AS (INSERT INTO imports (user_id, source, original_size) \
                    VALUES ('{user_id}', 'mal_xml', 0) RETURNING id), \
              item AS (INSERT INTO import_items (import_id, row_number, raw) \
                       SELECT id, 1, '{{}}' FROM i RETURNING import_id) \
         INSERT INTO jobs (job_type, payload) \
         SELECT 'import', jsonb_build_object('import_id', import_id) FROM item;
         WITH c AS (INSERT INTO notification_channels (user_id, channel_type, name) \
                    VALUES ('{user_id}', 'webhook', 'hook') RETURNING id) \
         INSERT INTO notification_subscriptions (user_id, channel_id, event_type) \
         SELECT '{user_id}', id, 'new_episode' FROM c;
         INSERT INTO jobs (job_type, payload) \
         VALUES ('notification', jsonb_build_object('user_id', '{user_id}'));"
    ))
    .await
    .unwrap();
}

/// Rows of `user_id`'s in `table`, through its owner's `user_id` column or
/// the parent row that has one.
async fn owned(db: &sea_orm::DatabaseConnection, table: &str, user_id: uuid::Uuid) -> i64 {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let entries = format!("SELECT id FROM library_entries WHERE user_id = '{user_id}'");
    let filter = match table {
        "users" => format!("id = '{user_id}'"),
        "entry_tags" | "watch_runs" => format!("entry_id IN ({entries})"),
        "plays" => format!("run_id IN (SELECT id FROM watch_runs WHERE entry_id IN ({entries}))"),
        "list_items" => {
            format!("list_id IN (SELECT id FROM lists WHERE user_id = '{user_id}')")
        }
        "import_items" => {
            format!("import_id IN (SELECT id FROM imports WHERE user_id = '{user_id}')")
        }
        "jobs" => format!(
            "payload->>'user_id' = '{user_id}' OR payload->>'import_id' IN \
             (SELECT id::text FROM imports WHERE user_id = '{user_id}')"
        ),
        _ => format!("user_id = '{user_id}'"),
    };
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT count(*) AS n FROM {table} WHERE {filter}"),
        ))
        .await
        .unwrap()
        .unwrap();
    row.try_get::<i64>("", "n").unwrap()
}

/// Every table a user's rows live in: each one with a `user_id` column (read
/// from the schema, so a new one is checked without editing this test), and
/// the children that hang off one.
async fn user_tables(db: &sea_orm::DatabaseConnection) -> Vec<String> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT table_name::text AS t FROM information_schema.columns \
             WHERE table_schema = 'public' AND column_name = 'user_id' \
             AND table_name IN (SELECT table_name FROM information_schema.tables \
                                WHERE table_type = 'BASE TABLE')",
        ))
        .await
        .unwrap();
    let mut tables: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String>("", "t").unwrap())
        .collect();
    for child in [
        "users",
        "entry_tags",
        "watch_runs",
        "plays",
        "list_items",
        "import_items",
        "jobs",
    ] {
        tables.push(child.to_owned());
    }
    tables.sort();
    tables.dedup();
    tables
}

#[tokio::test]
async fn purge_needs_both_confirmations_then_removes_everything() {
    let t = test_db!();
    let db = &t.db;
    let admin = common::user(db).await;
    let user = common::user(db).await;
    let bystander = common::user(db).await;
    populate(db, user.id).await;
    credentials_and_ops(db, user.id).await;
    credentials_and_ops(db, bystander.id).await;
    let svc = UserService::new(db.clone());
    let confirm = PurgeConfirmation {
        user_id: user.id,
        confirmed_id: user.id,
    };

    assert!(matches!(
        purge(db, admin.id, confirm).await,
        Err(UserError::PurgeRequiresDeactivation)
    ));
    svc.deactivate(user.id).await.unwrap();
    let wrong = PurgeConfirmation {
        confirmed_id: bystander.id,
        ..confirm
    };
    assert!(matches!(
        purge(db, admin.id, wrong).await,
        Err(UserError::PurgeNotConfirmed)
    ));
    // Not even a deactivated administrator's own account.
    assert!(matches!(
        purge(db, user.id, confirm).await,
        Err(UserError::CannotPurgeOwnAccount)
    ));
    let nobody = uuid::Uuid::now_v7();
    assert!(matches!(
        purge(
            db,
            admin.id,
            PurgeConfirmation {
                user_id: nobody,
                confirmed_id: nobody
            }
        )
        .await,
        Err(UserError::NotFound)
    ));
    assert_eq!(common::count(db, "users").await, 3);

    let tables = user_tables(db).await;
    for table in &tables {
        assert!(
            owned(db, table, user.id).await > 0,
            "{table} was not populated"
        );
    }
    let bystanders = {
        let mut counts = Vec::new();
        for table in &tables {
            counts.push(owned(db, table, bystander.id).await);
        }
        counts
    };

    let log = Log::default();
    {
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(log.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        purge(db, admin.id, confirm).await.unwrap();
    }
    // The audit line names both accounts by id, and nothing personal.
    let log = log.text();
    let audit = log
        .lines()
        .find(|l| l.contains("user purged"))
        .expect("an audit line");
    assert!(audit.contains(&admin.id.to_string()) && audit.contains(&user.id.to_string()));
    for personal in [
        &user.display_name,
        user.email.as_ref().unwrap(),
        &admin.display_name,
    ] {
        assert!(
            !log.contains(personal.as_str()),
            "{personal} reached the log"
        );
    }
    assert!(matches!(
        svc.get_by_id(user.id).await,
        Err(UserError::NotFound)
    ));
    for table in &tables {
        assert_eq!(owned(db, table, user.id).await, 0, "{table} not purged");
    }
    // Nobody else loses anything.
    for (table, before) in tables.iter().zip(bystanders) {
        assert_eq!(
            owned(db, table, bystander.id).await,
            before,
            "{table} lost a bystander's rows"
        );
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
    let window = LibraryService::new(db.clone())
        .list(user.id, &Default::default(), Default::default(), None, 50)
        .await
        .unwrap();
    assert_eq!((window.items.len(), window.more), (1, false));
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
