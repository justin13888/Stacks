//! An ephemeral PostgreSQL 18 database per test.
//!
//! Created from `DATABASE_URL`, migrated with the real baseline migration,
//! and dropped (with the degraded role, when one was made) when the
//! [`TestDb`] goes out of scope, even when the test panics. Without
//! `DATABASE_URL` the DB-backed tests skip; under CI a missing one fails.

use migration::MigratorTrait;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use uuid::Uuid;

pub struct TestDb {
    pub db: DatabaseConnection,
    name: String,
    admin_url: String,
    roles: std::sync::Mutex<Vec<String>>,
}

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ if std::env::var_os("CI").is_some() => {
            panic!("DATABASE_URL must be set in CI: the API tests need PostgreSQL 18")
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

fn with_credentials(url: &str, user: &str, password: &str) -> String {
    let mut u = reqwest::Url::parse(url).expect("DATABASE_URL is a URL");
    u.set_username(user).expect("a URL with a host");
    u.set_password(Some(password)).expect("a URL with a host");
    u.to_string()
}

async fn pool(url: &str) -> DatabaseConnection {
    let mut opts = ConnectOptions::new(url);
    opts.max_connections(8).sqlx_logging(false);
    Database::connect(opts)
        .await
        .expect("connect to the test database")
}

impl TestDb {
    pub async fn new() -> Option<Self> {
        let admin_url = database_url()?;
        let name = format!("stacks_api_test_{}", Uuid::now_v7().simple());
        let admin = Database::connect(&admin_url)
            .await
            .expect("connect to DATABASE_URL");
        admin
            .execute_unprepared(&format!("CREATE DATABASE \"{name}\""))
            .await
            .expect("create the test database");
        admin.close().await.ok();

        let db = pool(&with_database(&admin_url, &name)).await;
        migration::Migrator::up(&db, None)
            .await
            .expect("the baseline migration applies to an empty database");
        Some(Self {
            db,
            name,
            admin_url,
            roles: std::sync::Mutex::new(Vec::new()),
        })
    }

    /// A pool on the same database whose role can authenticate one kind of
    /// credential and do nothing else.
    ///
    /// A session-only pool can read accounts and sessions (and refresh a
    /// session's `last_seen_at`); a token-only pool can read accounts and
    /// tokens (and refresh `last_used_at`). The service's credential store
    /// built on one authenticates that kind of caller and then fails every
    /// other query it makes, which is how an operation that only touches the
    /// credential tables reaches its declared 500.
    pub async fn restricted_pool(&self, credential: Credential) -> DatabaseConnection {
        let role = format!("degraded_{}", Uuid::now_v7().simple());
        let password = "degraded";
        let grants = match credential {
            Credential::Session => {
                "GRANT SELECT ON users, sessions TO \"{role}\";
                 GRANT UPDATE (last_seen_at) ON sessions TO \"{role}\";"
            }
            Credential::Token => {
                "GRANT SELECT ON users, api_tokens TO \"{role}\";
                 GRANT UPDATE (last_used_at) ON api_tokens TO \"{role}\";"
            }
        }
        .replace("{role}", &role);
        let sql = format!(
            "CREATE ROLE \"{role}\" LOGIN PASSWORD '{password}';
             GRANT CONNECT ON DATABASE \"{db}\" TO \"{role}\";
             GRANT USAGE ON SCHEMA public TO \"{role}\";
             {grants}",
            db = self.name,
        );
        self.db
            .execute_unprepared(&sql)
            .await
            .expect("create a restricted role (DATABASE_URL must be a superuser)");
        self.roles.lock().expect("role lock").push(role.clone());
        let url = with_credentials(&with_database(&self.admin_url, &self.name), &role, password);
        pool(&url).await
    }

    /// A pool on the same database that has been closed: every query fails.
    pub async fn closed_pool(&self) -> DatabaseConnection {
        let closed = pool(&with_database(&self.admin_url, &self.name)).await;
        closed.clone().close().await.expect("close the pool");
        closed
    }
}

/// Which credential a restricted pool can authenticate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Credential {
    Session,
    Token,
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let admin_url = self.admin_url.clone();
        let name = self.name.clone();
        let roles = self.roles.lock().map(|r| r.clone()).unwrap_or_default();
        // Drop runs outside any async context it can await in, so the
        // teardown gets a short-lived runtime of its own.
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
                    for role in roles {
                        let _ = admin
                            .execute_unprepared(&format!("DROP ROLE IF EXISTS \"{role}\""))
                            .await;
                    }
                    admin.close().await.ok();
                }
            });
        });
        let _ = handle.join();
    }
}
