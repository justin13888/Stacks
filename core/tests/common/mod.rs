//! Shared harness for DB-backed tests.
//!
//! Each test gets its own ephemeral database, created from `DATABASE_URL` (a
//! PostgreSQL 18 server; CI provides one), migrated with the real baseline
//! migration, and dropped when the [`TestDb`] goes out of scope — even when
//! the test panics.
//!
//! Without `DATABASE_URL` the DB tests skip with a note, so `cargo test`
//! still runs everywhere. Under CI (`CI` set) a missing `DATABASE_URL` is a
//! failure instead, so the suite can never silently stop running there.

#![allow(dead_code)]

use migration::MigratorTrait;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use stacks_core::entity::enums::{MediaFormat, MediaKind};
use stacks_core::entity::{media, user};
use stacks_core::media::catalog::{self, MediaSnapshot};
use stacks_core::media::{MediaId, Provider};
use uuid::Uuid;

pub struct TestDb {
    pub db: DatabaseConnection,
    name: String,
    admin_url: String,
}

/// Get a fresh migrated database, or return from the test when none is
/// configured.
#[macro_export]
macro_rules! test_db {
    () => {
        match common::TestDb::new().await {
            Some(db) => db,
            None => return,
        }
    };
}

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ if std::env::var_os("CI").is_some() => {
            panic!("DATABASE_URL must be set in CI: the DB-backed tests need PostgreSQL 18")
        }
        _ => {
            eprintln!("DATABASE_URL not set; skipping DB-backed test");
            None
        }
    }
}

fn with_database(url: &str, name: &str) -> String {
    let mut u = reqwest::Url::parse(url).expect("DATABASE_URL is a URL");
    u.set_path(&format!("/{name}"));
    u.to_string()
}

impl TestDb {
    pub async fn new() -> Option<Self> {
        let admin_url = database_url()?;
        let name = format!("stacks_test_{}", Uuid::now_v7().simple());
        let admin = Database::connect(&admin_url)
            .await
            .expect("connect to DATABASE_URL");
        admin
            .execute_unprepared(&format!("CREATE DATABASE \"{name}\""))
            .await
            .expect("create test database");
        admin.close().await.ok();

        let mut opts = ConnectOptions::new(with_database(&admin_url, &name));
        opts.max_connections(8).sqlx_logging(false);
        let db = Database::connect(opts).await.expect("connect to test db");
        migration::Migrator::up(&db, None)
            .await
            .expect("baseline migration applies to an empty database");
        Some(Self {
            db,
            name,
            admin_url,
        })
    }

    /// A second, independent connection pool to the same database.
    pub async fn second_pool(&self) -> DatabaseConnection {
        let mut opts = ConnectOptions::new(with_database(&self.admin_url, &self.name));
        opts.max_connections(8).sqlx_logging(false);
        Database::connect(opts).await.expect("second pool")
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let admin_url = self.admin_url.clone();
        let name = self.name.clone();
        // Drop runs outside any async context we can await in, so tear the
        // database down on a short-lived runtime of its own.
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(async {
                if let Ok(admin) = Database::connect(&admin_url).await {
                    let _ = admin
                        .execute_unprepared(&format!(
                            "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
                        ))
                        .await;
                    admin.close().await.ok();
                }
            });
        });
        let _ = handle.join();
    }
}

// ------------------------------------------------------------------ fixtures

pub async fn user(db: &DatabaseConnection) -> user::Model {
    use sea_orm::ActiveModelTrait;
    use sea_orm::ActiveValue::{NotSet, Set};
    let id = Uuid::now_v7();
    user::ActiveModel {
        id: Set(id),
        oidc_issuer: Set("https://idp.test".into()),
        oidc_subject: Set(id.to_string()),
        email: Set(Some(format!("{id}@example.test"))),
        display_name: Set(format!("user-{}", &id.simple().to_string()[..8])),
        role: Set(stacks_core::entity::enums::Role::User),
        settings: Set(serde_json::json!({})),
        deactivated_at: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(db)
    .await
    .expect("insert user")
}

pub fn snapshot(kind: MediaKind, numeric: i32, title: &str) -> MediaSnapshot {
    let mut s = MediaSnapshot::placeholder(&MediaId::new(Provider::Tmdb, kind, numeric));
    s.title = title.to_owned();
    s.format = Some(match kind {
        MediaKind::Movie => MediaFormat::Movie,
        MediaKind::Show => MediaFormat::Tv,
    });
    s.runtime_minutes = Some(match kind {
        MediaKind::Movie => 120,
        MediaKind::Show => 45,
    });
    s.genres = vec!["Drama".into()];
    s
}

pub async fn movie(db: &DatabaseConnection, numeric: i32) -> media::Model {
    catalog::upsert_snapshot(db, &snapshot(MediaKind::Movie, numeric, "A Movie"))
        .await
        .expect("insert movie")
}

pub async fn show(db: &DatabaseConnection, numeric: i32) -> media::Model {
    catalog::upsert_snapshot(db, &snapshot(MediaKind::Show, numeric, "A Show"))
        .await
        .expect("insert show")
}

/// Count rows of a table.
pub async fn count(db: &DatabaseConnection, table: &str) -> i64 {
    use sea_orm::{DbBackend, Statement};
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT count(*) AS n FROM {table}"),
        ))
        .await
        .expect("count query")
        .expect("one row");
    row.try_get::<i64>("", "n").expect("count column")
}

/// Make every `entry_events` insert fail, to prove a mutation and its event
/// commit or roll back together.
pub async fn break_event_log(db: &DatabaseConnection) {
    db.execute_unprepared(
        "CREATE FUNCTION test_fail_event() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected event-log failure'; END $$; \
         CREATE TRIGGER test_fail_event BEFORE INSERT ON entry_events \
         FOR EACH ROW EXECUTE FUNCTION test_fail_event();",
    )
    .await
    .expect("install failing trigger");
}

pub async fn repair_event_log(db: &DatabaseConnection) {
    db.execute_unprepared(
        "DROP TRIGGER test_fail_event ON entry_events; DROP FUNCTION test_fail_event();",
    )
    .await
    .expect("remove failing trigger");
}
