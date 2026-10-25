//! The shared harness for the API's TestClient suites.
//!
//! An [`Env`] is one ephemeral database, one scripted metadata provider and
//! a [`Coverage`] every client it builds records into. Each area's suite
//! drives its operations, then asserts that every response its operations
//! declare was observed ([`Env::assert_covered`]) and that every observed
//! response conformed to the description (each client's
//! `assert_conformance`).
//!
//! The statuses every operation shares (401, 403, 406, 429, the extractor
//! 400s, 413, 415 and 422, and the 500 of a failing database) are driven
//! generically by [`sweep::sweep`], from the document itself.
//!
//! The API's suites include this directory as their `support` module; it is
//! also the `stacks-test-support` package (`Cargo.toml` beside this file),
//! which other crates, such as the e2e journeys, declare as a dependency.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

pub mod db;
pub mod example;
pub mod idp;
pub mod metadata;
pub mod sweep;

use std::sync::Arc;

use api::app::{App, Settings};
use kynos::openapi::{Document, Method, SpecVersion};
use kynos::test::{Coverage, TestClient, TestRequest};
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, DatabaseConnection};
use stacks_core::auth::AuthService;
use stacks_core::auth::oidc::{OidcHttp, OidcProvider};
use stacks_core::config::models::{AuthConfig, OidcConfig};
use stacks_core::entity::enums::{Role, Scope};
use stacks_core::entity::{session, user};
use uuid::Uuid;

pub use metadata::MockMetadata;

/// Skip the test when no database is configured.
#[macro_export]
macro_rules! env {
    () => {
        match support::Env::new().await {
            Some(env) => env,
            None => return,
        }
    };
}

/// The base every problem type shares.
pub const PROBLEMS: &str = api::problem::BASE;

pub fn problem(slug: &str) -> String {
    format!("{PROBLEMS}{slug}")
}

/// Settings for tests: a rate limit no suite reaches.
pub fn settings() -> Settings {
    Settings {
        secure_cookies: false,
        rate_limit_per_minute: 1_000_000,
        ..Settings::default()
    }
}

pub struct Env {
    pub db: db::TestDb,
    pub metadata: MockMetadata,
    pub coverage: Coverage,
    pub document: Document,
    oidc: OidcConfig,
}

/// A signed-in account and its credentials.
#[derive(Clone, Debug)]
pub struct TestUser {
    pub user: user::Model,
    /// The session cookie's value.
    pub session: String,
    /// A token holding `read` only.
    pub read: String,
    /// A token holding `write` only.
    pub write: String,
    /// A token holding `read` and `write`.
    pub read_write: String,
    /// A token holding `admin`, for an administrator.
    pub admin: Option<String>,
}

impl TestUser {
    pub fn id(&self) -> Uuid {
        self.user.id
    }
}

/// Credentials on a request.
pub trait Credentials: Sized {
    fn session(self, user: &TestUser) -> Self;
    fn bearer(self, token: &str) -> Self;
}

impl<C> Credentials for TestRequest<'_, C> {
    fn session(self, user: &TestUser) -> Self {
        self.cookie(api::security::SESSION_COOKIE, &user.session)
    }

    fn bearer(self, token: &str) -> Self {
        self.header("authorization", &format!("Bearer {token}"))
    }
}

impl Env {
    pub async fn new() -> Option<Self> {
        Self::with_issuer("http://127.0.0.1:9").await
    }

    /// An environment whose sign-ins go to `issuer`.
    pub async fn with_issuer(issuer: &str) -> Option<Self> {
        let db = db::TestDb::new().await?;
        let document = api::app::router(&settings())
            .openapi_as(SpecVersion::V3_1)
            .expect("the 3.1 document");
        Some(Self {
            db,
            metadata: MockMetadata::default(),
            coverage: Coverage::new(),
            document,
            oidc: OidcConfig {
                issuer: issuer.to_owned(),
                client_id: idp::CLIENT_ID.to_owned(),
                client_secret: idp::CLIENT_SECRET.to_owned(),
                redirect_uri: idp::REDIRECT_URI.to_owned(),
                internal_issuer: None,
            },
        })
    }

    pub fn auth(&self, db: &DatabaseConnection) -> AuthService {
        self.auth_against(db, &self.oidc.issuer)
    }

    fn auth_against(&self, db: &DatabaseConnection, issuer: &str) -> AuthService {
        let http = OidcHttp::new(std::time::Duration::from_secs(5)).expect("an HTTP client");
        AuthService::new(
            db.clone(),
            AuthConfig {
                session_ttl_secs: 3600,
                admin_emails: vec!["root@example.test".into()],
            },
            Arc::new(OidcProvider::new(
                OidcConfig {
                    issuer: issuer.to_owned(),
                    ..self.oidc.clone()
                },
                http,
            )),
        )
    }

    fn build(&self, app: App, settings: &Settings) -> TestClient<App> {
        let service = api::app::router(settings)
            .build(app)
            .expect("the router builds");
        TestClient::new(service).with_coverage(&self.coverage)
    }

    fn app(&self, db: DatabaseConnection, auth: AuthService, settings: Settings) -> App {
        App {
            auth: Arc::new(auth),
            db,
            metadata: Arc::new(self.metadata.clone()),
            settings: Arc::new(settings),
        }
    }

    /// A client of the service as deployed, recording into the coverage.
    pub fn client(&self) -> TestClient<App> {
        self.client_with(settings())
    }

    pub fn client_with(&self, settings: Settings) -> TestClient<App> {
        let db = self.db.db.clone();
        let app = self.app(db.clone(), self.auth(&db), settings.clone());
        self.build(app, &settings)
    }

    /// A client whose sign-ins go to `issuer` instead.
    pub fn client_with_issuer(&self, issuer: &str) -> TestClient<App> {
        let db = self.db.db.clone();
        let app = self.app(db.clone(), self.auth_against(&db, issuer), settings());
        self.build(app, &settings())
    }

    /// A client of a service whose database fails every query, except that
    /// its credential store can still authenticate `credential` callers.
    ///
    /// Every operation's own queries fail, so this is how each declared 500
    /// is exercised without a fault hook in the server.
    pub async fn degraded_client(&self, credential: db::Credential) -> TestClient<App> {
        let restricted = self.db.restricted_pool(credential).await;
        let app = self.app(
            self.db.closed_pool().await,
            self.auth(&restricted),
            settings(),
        );
        self.build(app, &settings())
    }

    /// A client of a service whose database connection is closed, credential
    /// store included.
    pub async fn closed_client(&self) -> TestClient<App> {
        let closed = self.db.closed_pool().await;
        let app = self.app(closed.clone(), self.auth(&closed), settings());
        self.build(app, &settings())
    }

    /// A client of a service allowing one request a minute.
    pub fn limited_client(&self) -> TestClient<App> {
        self.client_with(Settings {
            rate_limit_per_minute: 1,
            ..settings()
        })
    }

    /// A new account with a session and one token per scope set.
    pub async fn user(&self, role: Role) -> TestUser {
        let db = &self.db.db;
        let id = Uuid::now_v7();
        let user = user::ActiveModel {
            id: Set(id),
            oidc_issuer: Set(self.oidc.issuer.clone()),
            oidc_subject: Set(id.to_string()),
            email: Set(Some(format!("{id}@example.test"))),
            display_name: Set(format!("user-{}", &id.simple().to_string()[..8])),
            role: Set(role),
            settings: Set(serde_json::json!({})),
            deactivated_at: Set(None),
            created_at: NotSet,
            updated_at: NotSet,
        }
        .insert(db)
        .await
        .expect("insert a user");
        let (session, hash) = stacks_core::auth::session::generate();
        session::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user.id),
            token_hash: Set(hash),
            user_agent: Set(Some("test".into())),
            ip: Set(None),
            created_at: NotSet,
            last_seen_at: NotSet,
            expires_at: Set((chrono::Utc::now() + chrono::Duration::hours(1)).into()),
        }
        .insert(db)
        .await
        .expect("insert a session");
        let auth = self.auth(db);
        let token = |scopes: &'static [Scope]| {
            let auth = &auth;
            let user = &user;
            async move {
                auth.create_api_token(user, "test", scopes, None)
                    .await
                    .expect("create a token")
                    .1
            }
        };
        let read = token(&[Scope::Read]).await;
        let write = token(&[Scope::Write]).await;
        let read_write = token(&[Scope::Read, Scope::Write]).await;
        let admin = if role == Role::Admin {
            Some(token(&[Scope::Read, Scope::Write, Scope::Admin]).await)
        } else {
            None
        };
        TestUser {
            user,
            session,
            read,
            write,
            read_write,
            admin,
        }
    }

    /// Deactivate an account, so its credentials are genuine and refused.
    pub async fn deactivate(&self, user: &TestUser) {
        stacks_core::user::UserService::new(self.db.db.clone())
            .deactivate(user.id())
            .await
            .expect("deactivate");
    }

    /// The document narrowed to the operations carrying `tag`.
    pub fn area(&self, tag: &str) -> Document {
        let mut narrowed = self.document.clone();
        for item in narrowed.paths.items.values_mut() {
            for method in Method::all() {
                let keep = item
                    .operation(*method)
                    .is_some_and(|op| op.tags.iter().any(|t| t == tag));
                if !keep {
                    clear(item, *method);
                }
            }
        }
        narrowed
            .paths
            .items
            .retain(|_, item| item.operations().next().is_some());
        narrowed
    }

    /// Every response the operations tagged `tag` declare was observed by
    /// some client of this environment.
    pub fn assert_covered(&self, tag: &str) {
        let area = self.area(tag);
        assert!(
            !area.paths.items.is_empty(),
            "no operation carries the tag `{tag}`"
        );
        self.coverage.assert_declared_responses_covered(&area);
    }
}

fn clear(item: &mut kynos::openapi::PathItem, method: Method) {
    match method {
        Method::Get => item.get = None,
        Method::Put => item.put = None,
        Method::Post => item.post = None,
        Method::Delete => item.delete = None,
        Method::Options => item.options = None,
        Method::Head => item.head = None,
        Method::Patch => item.patch = None,
        Method::Trace => item.trace = None,
        // Methods only OpenAPI 3.2 has; the document is 3.1.
        _ => {}
    }
}
