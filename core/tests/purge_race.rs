//! The purge against concurrent writers, forced deterministically against
//! PostgreSQL 18 with a second connection.
//!
//! - A reactivation, committed before the purge or still uncommitted while
//!   it runs, is seen and refuses the purge: the purge locks the user's row
//!   before checking it.
//! - A metadata refresh that announces a new episode while a purge is in
//!   flight queues no notification naming the purged user, whichever of the
//!   two reaches the user's row first.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Duration as Days, NaiveDate, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Statement,
    TransactionTrait,
};
use stacks_core::entity::enums::{Category, EntryStatus, JobType};
use stacks_core::entity::{job, user};
use stacks_core::jobs::JobContext;
use stacks_core::jobs::handlers::metadata_refresh::{self, MetadataRefreshPayload};
use stacks_core::library::{EntryPatch, LibraryService};
use stacks_core::media::catalog::MediaSnapshot;
use stacks_core::media::models::{
    EpisodeInfo, Freshness, MediaDetails, MediaWithUserState, SeasonInfo,
};
use stacks_core::media::provider::MetadataFuture;
use stacks_core::media::{
    DiscoverCategory, MediaId, MediaKind, MetadataError, MetadataErrorKind, MetadataPage,
    MetadataProvider, Operation, Provider, SeasonDetails, TitleDetails,
};
use stacks_core::user::{PurgeConfirmation, UserError, UserService, purge};
use uuid::Uuid;

/// Wait (bounded) until at least `n` backends are blocked on a lock.
async fn until_blocked(db: &DatabaseConnection, n: i64, what: &str) {
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
        if row.try_get::<i64>("", "n").unwrap() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{what}");
}

fn confirm(id: Uuid) -> PurgeConfirmation {
    PurgeConfirmation {
        user_id: id,
        confirmed_id: id,
    }
}

async fn deactivated(db: &DatabaseConnection) -> user::Model {
    let target = common::user(db).await;
    UserService::new(db.clone())
        .deactivate(target.id)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_reactivation_committed_before_the_purge_refuses_it() {
    let t = test_db!();
    let admin = common::user(&t.db).await;
    let target = deactivated(&t.db).await;
    let other = t.second_pool().await;
    UserService::new(other).reactivate(target.id).await.unwrap();

    let outcome = purge(&t.db, admin.id, confirm(target.id)).await;
    assert!(
        matches!(outcome, Err(UserError::PurgeRequiresDeactivation)),
        "{outcome:?}"
    );
    assert_eq!(common::count(&t.db, "users").await, 2);
}

#[tokio::test]
async fn a_reactivation_in_flight_during_the_purge_refuses_it() {
    let t = test_db!();
    let admin = common::user(&t.db).await;
    let target = deactivated(&t.db).await;
    let other = t.second_pool().await;

    // The reactivation holds the user's row, uncommitted.
    let reactivation = other.begin().await.unwrap();
    reactivation
        .execute_unprepared(&format!(
            "UPDATE users SET deactivated_at = NULL WHERE id = '{}'",
            target.id
        ))
        .await
        .unwrap();

    let db = t.db.clone();
    let (actor, id) = (admin.id, target.id);
    let purging = tokio::spawn(async move { purge(&db, actor, confirm(id)).await });
    until_blocked(&other, 1, "the purge never waited on the reactivation").await;
    reactivation.commit().await.unwrap();

    let outcome = purging.await.unwrap();
    assert!(
        matches!(outcome, Err(UserError::PurgeRequiresDeactivation)),
        "the purge checked a stale row: {outcome:?}"
    );
    let after = user::Entity::find_by_id(target.id)
        .one(&t.db)
        .await
        .unwrap()
        .expect("the reactivated account survives");
    assert!(after.deactivated_at.is_none());
}

// ------------------------------------------- a refresh racing the purge

/// One season's episodes: number and air date.
type Episodes = Vec<(u32, Option<NaiveDate>)>;

/// A show whose one season lists `episodes`.
#[derive(Clone, Default)]
struct Mock(Arc<Mutex<Episodes>>);

impl Mock {
    fn episodes(&self) -> Episodes {
        self.0.lock().expect("mock lock").clone()
    }

    fn season_info(&self) -> SeasonInfo {
        SeasonInfo {
            season_number: 1,
            name: Some("Season 1".into()),
            overview: None,
            air_date: None,
            poster_url: None,
            episode_count: u32::try_from(self.episodes().len()).ok(),
        }
    }

    fn empty_page() -> Result<MetadataPage<MediaWithUserState>, MetadataError> {
        Ok(MetadataPage {
            items: Vec::new(),
            page: 1,
            has_next: false,
            unavailable: Vec::new(),
        })
    }
}

impl MetadataProvider for Mock {
    fn search<'a>(
        &'a self,
        _provider: Provider,
        _query: &'a str,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { Self::empty_page() })
    }

    fn details<'a>(&'a self, id: &'a MediaId) -> MetadataFuture<'a, TitleDetails> {
        Box::pin(async move {
            let mut snapshot = MediaSnapshot::placeholder(id);
            snapshot.title = "A Show".into();
            snapshot.season_count = Some(1);
            Ok(TitleDetails {
                detail: MediaDetails {
                    media: MediaWithUserState {
                        cameo_media_id: id.to_string(),
                        title: "A Show".into(),
                        original_title: None,
                        romaji_title: None,
                        kind: id.kind,
                        category: Category::Series,
                        year: None,
                        overview: None,
                        poster_url: None,
                        backdrop_url: None,
                        genres: Vec::new(),
                        rating: None,
                        episode_count: None,
                        watch_status: None,
                        rewatching: false,
                        score: None,
                        is_favorite: false,
                    },
                    format: None,
                    tagline: None,
                    runtime_minutes: None,
                    status: None,
                    homepage: None,
                    imdb_id: None,
                    number_of_seasons: Some(1),
                    in_production: None,
                    freshness: Freshness::live(),
                },
                snapshot,
                seasons: vec![self.season_info()],
            })
        })
    }

    fn season<'a>(&'a self, id: &'a MediaId, season: u32) -> MetadataFuture<'a, SeasonDetails> {
        Box::pin(async move {
            if season != 1 {
                return Err(MetadataError::new(
                    id.provider,
                    Operation::Season,
                    MetadataErrorKind::NotFound,
                ));
            }
            Ok(SeasonDetails {
                season: self.season_info(),
                episodes: self
                    .episodes()
                    .into_iter()
                    .map(|(n, aired)| EpisodeInfo {
                        season_number: 1,
                        episode_number: n,
                        absolute_number: None,
                        name: None,
                        overview: None,
                        air_date: aired.map(|d| d.to_string()),
                        runtime_minutes: None,
                        still_url: None,
                        rating: None,
                    })
                    .collect(),
            })
        })
    }

    fn recommendations<'a>(
        &'a self,
        _id: &'a MediaId,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { Self::empty_page() })
    }

    fn feed<'a>(
        &'a self,
        _provider: Provider,
        _feed: DiscoverCategory,
        _kind: MediaKind,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move { Self::empty_page() })
    }
}

fn days_ago(n: i64) -> Option<NaiveDate> {
    Some(Utc::now().date_naive() - Days::days(n))
}

/// A show both users watch, its first scan done, and a new episode aired
/// since: the next refresh announces it to each watcher. `target` is
/// deactivated, ready to purge, and holds a session.
struct Race {
    t: common::TestDb,
    other: DatabaseConnection,
    admin: Uuid,
    target: Uuid,
    bystander: Uuid,
    ctx: JobContext,
    payload: MetadataRefreshPayload,
}

async fn race() -> Option<Race> {
    let t = common::TestDb::new().await?;
    let db = &t.db;
    let admin = common::user(db).await;
    let target = common::user(db).await;
    let bystander = common::user(db).await;
    let show = common::show(db, 1).await;
    let library = LibraryService::new(db.clone());
    for u in [&target, &bystander] {
        let watching = EntryPatch {
            status: Some(EntryStatus::Watching),
            ..Default::default()
        };
        library.upsert(u.id, show.id, watching).await.unwrap();
    }
    let hash = target.id.simple().to_string().repeat(2);
    db.execute_unprepared(&format!(
        "INSERT INTO sessions (user_id, token_hash, expires_at) \
         VALUES ('{}', '{hash}', now() + interval '1 day')",
        target.id
    ))
    .await
    .unwrap();
    UserService::new(db.clone())
        .deactivate(target.id)
        .await
        .unwrap();

    let mock = Mock::default();
    *mock.0.lock().unwrap() = vec![(1, days_ago(7))];
    let ctx = JobContext {
        db: db.clone(),
        metadata: Arc::new(mock.clone()),
    };
    let payload = MetadataRefreshPayload {
        media_ids: vec![show.id],
        batch_size: None,
    };
    let baseline = metadata_refresh::run(&ctx, payload.clone()).await.unwrap();
    assert_eq!(baseline.notifications, 0, "the first scan is the baseline");
    mock.0.lock().unwrap().push((2, days_ago(0)));
    let other = t.second_pool().await;
    Some(Race {
        t,
        other,
        admin: admin.id,
        target: target.id,
        bystander: bystander.id,
        ctx,
        payload,
    })
}

/// The users the queued `new_episode` notifications name.
async fn notified(db: &DatabaseConnection) -> Vec<Uuid> {
    job::Entity::find()
        .filter(job::Column::JobType.eq(JobType::Notification))
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|j| j.payload)
        .map(|p| p["user_id"].as_str().unwrap().parse().unwrap())
        .collect()
}

impl Race {
    async fn assert_purged_and_not_notified(&self) {
        let db = &self.t.db;
        assert!(
            user::Entity::find_by_id(self.target)
                .one(db)
                .await
                .unwrap()
                .is_none(),
            "the purge committed"
        );
        assert_eq!(
            notified(db).await,
            vec![self.bystander],
            "only the bystander's notification is queued"
        );
    }
}

#[tokio::test]
async fn a_refresh_during_the_purge_notifies_nobody_it_purged() {
    let Some(r) = race().await else {
        return;
    };
    // Hold the purge part-way: it has locked the user's row and is deleting
    // when it reaches the session this transaction holds.
    let hold = r.other.begin().await.unwrap();
    hold.execute_unprepared(&format!(
        "SELECT 1 FROM sessions WHERE user_id = '{}' FOR UPDATE",
        r.target
    ))
    .await
    .unwrap();
    let db = r.t.db.clone();
    let (actor, id) = (r.admin, r.target);
    let purging = tokio::spawn(async move { purge(&db, actor, confirm(id)).await });
    until_blocked(&r.other, 1, "the purge never reached the held session").await;

    let (ctx, payload) = (r.ctx.clone(), r.payload.clone());
    let refreshing = tokio::spawn(async move { metadata_refresh::run(&ctx, payload).await });
    until_blocked(&r.other, 2, "the refresh never waited on the purge").await;
    hold.rollback().await.unwrap();

    purging.await.unwrap().unwrap();
    let report = refreshing.await.unwrap().unwrap();
    assert_eq!(report.notifications, 1, "the bystander is still told");
    r.assert_purged_and_not_notified().await;
}

#[tokio::test]
async fn a_purge_after_the_refresh_locked_its_watchers_deletes_their_notifications() {
    let Some(r) = race().await else {
        return;
    };
    // Queue the refresh, then the purge, behind one lock on the user's row,
    // so the refresh takes the row first and the purge waits for it.
    let hold = r.other.begin().await.unwrap();
    hold.execute_unprepared(&format!(
        "SELECT 1 FROM users WHERE id = '{}' FOR UPDATE",
        r.target
    ))
    .await
    .unwrap();
    let (ctx, payload) = (r.ctx.clone(), r.payload.clone());
    let refreshing = tokio::spawn(async move { metadata_refresh::run(&ctx, payload).await });
    until_blocked(&r.other, 1, "the refresh never locked its watchers").await;
    let db = r.t.db.clone();
    let (actor, id) = (r.admin, r.target);
    let purging = tokio::spawn(async move { purge(&db, actor, confirm(id)).await });
    until_blocked(&r.other, 2, "the purge never waited on the user's row").await;
    hold.rollback().await.unwrap();

    refreshing.await.unwrap().unwrap();
    purging.await.unwrap().unwrap();
    r.assert_purged_and_not_notified().await;
}
