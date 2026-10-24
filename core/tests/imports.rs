//! Third-party imports against PostgreSQL 18 with a scripted metadata
//! provider: submitting, matching (by id, by AniList's MAL link, by title
//! search), unmatched rows and resolving them, the conflict strategies, the
//! plays and runs a row becomes, and surviving a metadata outage.

mod common;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use stacks_core::entity::enums::{
    ConflictStrategy, DatePrecision, EntryStatus, ImportOutcome, ImportSource, ImportStatus,
    JobStatus, PlaySource, RefProvider,
};
use stacks_core::entity::{import_item, job};
use stacks_core::import_export::ImportExportError;
use stacks_core::import_export::import::matching::{self, Candidate};
use stacks_core::import_export::import::pipeline::{self, ResolveError};
use stacks_core::import_export::import::{ImportPipeline, mal, tmdb};
use stacks_core::jobs::{JobContext, JobRunner};
use stacks_core::library::LibraryService;
use stacks_core::media::catalog::{self, MediaSnapshot};
use stacks_core::media::models::{Freshness, MediaDetails, MediaWithUserState};
use stacks_core::media::provider::MetadataFuture;
use stacks_core::media::{
    DiscoverCategory, MediaId, MediaKind, MetadataError, MetadataErrorKind, MetadataPage,
    MetadataProvider, Operation, Provider, SeasonDetails, TitleDetails, UnavailableCause,
};
use uuid::Uuid;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/import_export");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).expect("fixture exists")
}

const TMDB: &[u8] = br#"[{"tmdb_id": 550, "title": "Fight Club", "media_type": "movie"}]"#;

// ------------------------------------------------------------------ the mock

/// A known title: its name, year and episode count.
type Known = (&'static str, i32, Option<i32>);

#[derive(Default)]
struct Script {
    titles: HashMap<MediaId, Known>,
    /// AniList's `idMal` links.
    mal: HashMap<i64, MediaId>,
    /// What a search for each query returns.
    searches: HashMap<String, Vec<MediaId>>,
    /// Queries whose search the provider rejects outright.
    broken: HashSet<String>,
    /// Every call fails with this, while set.
    down: Option<MetadataErrorKind>,
    /// When set, this many more calls answer; every one after is
    /// rate-limited.
    budget: Option<usize>,
    /// Every call the provider was asked to make.
    calls: usize,
    /// A MAL lookup of this id says it has begun (`entered`), then waits
    /// for `release`.
    gate: Option<(i64, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}

#[derive(Default, Clone)]
struct Mock(Arc<Mutex<Script>>);

fn id(raw: &str) -> MediaId {
    MediaId::parse(raw).expect("a fixture id")
}

impl Mock {
    fn script(&self) -> std::sync::MutexGuard<'_, Script> {
        self.0.lock().expect("script lock")
    }

    /// The titles the fixtures name, as AniList and TMDB know them.
    fn scripted() -> Self {
        let m = Self::default();
        {
            let mut s = m.script();
            for (raw, known) in [
                (
                    "anilist:show:30",
                    ("Neon Genesis Evangelion", 1995, Some(26)),
                ),
                ("anilist:show:21", ("One Piece", 1999, None)),
                (
                    "anilist:show:5114",
                    ("Fullmetal Alchemist: Brotherhood", 2009, Some(64)),
                ),
                ("anilist:show:121", ("Fullmetal Alchemist", 2003, Some(51))),
                ("anilist:show:270", ("Hellsing", 2001, Some(13))),
                ("anilist:show:777", ("Hellsing", 2006, Some(10))),
                ("anilist:show:1", ("Cowboy Bebop", 1998, Some(26))),
                ("tmdb:movie:550", ("Fight Club", 1999, None)),
                ("tmdb:movie:13", ("Forrest Gump", 1994, None)),
                ("tmdb:show:1396", ("Breaking Bad", 2008, Some(62))),
                ("tmdb:show:64010", ("Reply 1988", 2015, Some(20))),
            ] {
                s.titles.insert(id(raw), known);
            }
            for (mal, raw) in [
                (30, "anilist:show:30"),
                (21, "anilist:show:21"),
                (1, "anilist:show:1"),
            ] {
                s.mal.insert(mal, id(raw));
            }
            s.searches.insert(
                "Fullmetal Alchemist: Brotherhood".into(),
                vec![id("anilist:show:5114"), id("anilist:show:121")],
            );
            s.searches.insert(
                "Hellsing".into(),
                vec![id("anilist:show:270"), id("anilist:show:777")],
            );
            s.broken.insert("Upstream Breaks On This".into());
        }
        m
    }

    fn set_down(&self, kind: Option<MetadataErrorKind>) {
        self.script().down = kind;
    }

    fn check(&self, provider: Provider, op: Operation) -> Result<(), MetadataError> {
        let mut s = self.script();
        s.calls += 1;
        if let Some(left) = s.budget.as_mut() {
            if *left == 0 {
                return Err(MetadataError::new(
                    provider,
                    op,
                    MetadataErrorKind::RateLimited { retry_after: None },
                ));
            }
            *left -= 1;
        }
        match &s.down {
            Some(kind) => Err(MetadataError::new(provider, op, kind.clone())),
            None => Ok(()),
        }
    }

    fn item(id: &MediaId, (name, year, _): Known) -> MediaWithUserState {
        let mut snap = MediaSnapshot::placeholder(id);
        snap.title = name.to_owned();
        let mut item = MediaWithUserState::from_snapshot(
            &stacks_core::entity::media::Model {
                id: Uuid::nil(),
                kind: id.kind,
                format: None,
                category: stacks_core::entity::enums::Category::Anime,
                title: name.to_owned(),
                original_title: None,
                romaji_title: None,
                alt_titles: serde_json::json!([]),
                overview: None,
                poster_url: None,
                backdrop_url: None,
                start_date: NaiveDate::from_ymd_opt(year, 1, 1),
                end_date: None,
                status: None,
                runtime_minutes: None,
                genres: Vec::new(),
                origin_countries: Vec::new(),
                original_language: None,
                season_count: None,
                episode_count: None,
                raw: None,
                fetched_at: None,
                refresh_error: None,
                refresh_failures: 0,
                episodes_checked_at: None,
                created_at: chrono::Utc::now().into(),
                updated_at: chrono::Utc::now().into(),
            },
            id,
        );
        item.year = Some(year);
        item.poster_url = Some(format!("https://img.example.test/{id}.jpg"));
        item
    }
}

fn unavailable() -> MetadataErrorKind {
    MetadataErrorKind::Unavailable(UnavailableCause::Status(503))
}

impl MetadataProvider for Mock {
    fn search<'a>(
        &'a self,
        provider: Provider,
        query: &'a str,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move {
            self.check(provider, Operation::Search)?;
            let s = self.script();
            if s.broken.contains(query) {
                return Err(MetadataError::new(
                    provider,
                    Operation::Search,
                    MetadataErrorKind::Misconfigured("HTTP 400".into()),
                ));
            }
            let items = s
                .searches
                .get(query)
                .into_iter()
                .flatten()
                .filter(|id| id.provider == provider)
                .map(|id| Self::item(id, s.titles[id]))
                .collect();
            Ok(MetadataPage {
                items,
                page: 1,
                has_next: false,
                unavailable: Vec::new(),
            })
        })
    }

    fn details<'a>(&'a self, id: &'a MediaId) -> MetadataFuture<'a, TitleDetails> {
        Box::pin(async move {
            self.check(id.provider, Operation::Details)?;
            let Some(known) = self.script().titles.get(id).copied() else {
                return Err(MetadataError::new(
                    id.provider,
                    Operation::Details,
                    MetadataErrorKind::NotFound,
                ));
            };
            let mut snapshot = MediaSnapshot::placeholder(id);
            snapshot.title = known.0.to_owned();
            snapshot.start_date = NaiveDate::from_ymd_opt(known.1, 1, 1);
            snapshot.episode_count = known.2;
            Ok(TitleDetails {
                detail: MediaDetails {
                    media: Self::item(id, known),
                    format: None,
                    tagline: None,
                    runtime_minutes: None,
                    status: None,
                    homepage: None,
                    imdb_id: None,
                    number_of_seasons: None,
                    in_production: None,
                    freshness: Freshness::live(),
                },
                snapshot,
                seasons: Vec::new(),
            })
        })
    }

    fn season<'a>(&'a self, id: &'a MediaId, _season: u32) -> MetadataFuture<'a, SeasonDetails> {
        Box::pin(async move {
            Err(MetadataError::new(
                id.provider,
                Operation::Season,
                MetadataErrorKind::Unsupported,
            ))
        })
    }

    fn recommendations<'a>(
        &'a self,
        id: &'a MediaId,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move {
            Err(MetadataError::new(
                id.provider,
                Operation::Recommendations,
                MetadataErrorKind::Unsupported,
            ))
        })
    }

    fn feed<'a>(
        &'a self,
        provider: Provider,
        _feed: DiscoverCategory,
        _kind: MediaKind,
        _page: u32,
    ) -> MetadataFuture<'a, MetadataPage<MediaWithUserState>> {
        Box::pin(async move {
            Err(MetadataError::new(
                provider,
                Operation::Feed,
                MetadataErrorKind::Unsupported,
            ))
        })
    }

    fn anilist_by_mal_id(&self, mal_id: i64) -> MetadataFuture<'_, MediaId> {
        Box::pin(async move {
            let gate = self
                .script()
                .gate
                .clone()
                .filter(|(gated, _, _)| *gated == mal_id);
            if let Some((_, entered, release)) = gate {
                entered.notify_one();
                release.notified().await;
            }
            self.check(Provider::AniList, Operation::Extras)?;
            self.script().mal.get(&mal_id).cloned().ok_or_else(|| {
                MetadataError::new(
                    Provider::AniList,
                    Operation::Extras,
                    MetadataErrorKind::NotFound,
                )
            })
        })
    }
}

// ------------------------------------------------------------------ helpers

async fn submit(
    db: &sea_orm::DatabaseConnection,
    user: Uuid,
    source: ImportSource,
    file: &[u8],
    strategy: ConflictStrategy,
) -> Uuid {
    pipeline::submit(db, user, source, None, file.to_vec(), strategy, 1 << 20)
        .await
        .expect("the upload is accepted")
        .id
}

fn pipeline_for(db: &sea_orm::DatabaseConnection, mock: &Mock) -> ImportPipeline {
    ImportPipeline::new(db.clone(), Arc::new(mock.clone()))
}

async fn rows(db: &sea_orm::DatabaseConnection, import_id: Uuid) -> Vec<import_item::Model> {
    pipeline::items(db, import_id).await.unwrap()
}

fn outcomes(rows: &[import_item::Model]) -> Vec<ImportOutcome> {
    rows.iter().map(|r| r.outcome).collect()
}

async fn entry_for(
    db: &sea_orm::DatabaseConnection,
    user: Uuid,
    raw: &str,
) -> Option<stacks_core::library::EntryView> {
    let m = catalog::find_by_ref(db, &id(raw)).await.unwrap()?;
    LibraryService::new(db.clone())
        .get(user, m.id)
        .await
        .unwrap()
}

/// `total = applied + skipped + unmatched + failed + pending_retry`, on the
/// import row and in its rows.
async fn assert_accounted(db: &sea_orm::DatabaseConnection, user: Uuid, import_id: Uuid) {
    let imp = pipeline::find_for_user(db, user, import_id)
        .await
        .unwrap()
        .unwrap();
    let rows = rows(db, import_id).await;
    let waiting = rows
        .iter()
        .filter(|r| {
            matches!(
                r.outcome,
                ImportOutcome::Pending | ImportOutcome::PendingRetry
            )
        })
        .count() as i32;
    assert_eq!(imp.total_count, rows.len() as i32);
    assert_eq!(
        imp.total_count,
        imp.applied_count + imp.skipped_count + imp.unmatched_count + imp.failed_count + waiting
    );
    for r in rows.iter().filter(|r| r.outcome == ImportOutcome::Failed) {
        assert!(
            r.error.is_some(),
            "failed row {} has no reason",
            r.row_number
        );
    }
}

// ------------------------------------------------------------------ submission

#[tokio::test]
async fn an_unreadable_file_is_refused_before_anything_is_stored() {
    let t = test_db!();
    let user = common::user(&t.db).await;
    let refused = pipeline::submit(
        &t.db,
        user.id,
        ImportSource::MalXml,
        Some("export.json".into()),
        TMDB.to_vec(),
        ConflictStrategy::Skip,
        1024,
    )
    .await;
    assert!(matches!(refused, Err(ImportExportError::ParseError(_))));
    assert_eq!(common::count(&t.db, "imports").await, 0);
    assert_eq!(common::count(&t.db, "jobs").await, 0);

    let too_big = pipeline::submit(
        &t.db,
        user.id,
        ImportSource::TmdbJson,
        None,
        TMDB.to_vec(),
        ConflictStrategy::Skip,
        8,
    )
    .await;
    assert!(matches!(too_big, Err(ImportExportError::TooLarge { .. })));

    submit(
        &t.db,
        user.id,
        ImportSource::TmdbJson,
        TMDB,
        ConflictStrategy::Skip,
    )
    .await;
    assert_eq!(common::count(&t.db, "imports").await, 1);
    assert_eq!(common::count(&t.db, "jobs").await, 1);
}

#[tokio::test]
async fn listings_page_newest_first_without_the_files() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let eve = common::user(&t.db).await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        let row = pipeline::submit(
            &t.db,
            ada.id,
            ImportSource::TmdbJson,
            None,
            TMDB.to_vec(),
            ConflictStrategy::Merge,
            1024,
        )
        .await
        .unwrap();
        assert!(row.original_file.is_some(), "the file is kept");
        ids.push(row.id);
    }
    ids.reverse();

    let first = pipeline::list_for_user(&t.db, ada.id, None, 2)
        .await
        .unwrap();
    assert_eq!(first.iter().map(|i| i.id).collect::<Vec<_>>(), ids[..2]);
    assert!(first.iter().all(|i| i.original_file.is_none()));
    assert!(first.iter().all(|i| i.original_size == TMDB.len() as i64));
    let rest = pipeline::list_for_user(&t.db, ada.id, Some(ids[1]), 2)
        .await
        .unwrap();
    assert_eq!(rest.iter().map(|i| i.id).collect::<Vec<_>>(), ids[2..]);
    assert!(
        pipeline::list_for_user(&t.db, eve.id, None, 10)
            .await
            .unwrap()
            .is_empty()
    );

    let one = pipeline::find_for_user(&t.db, ada.id, ids[0])
        .await
        .unwrap()
        .expect("ada's import");
    assert!(one.original_file.is_none());
    assert_eq!(one.conflict_strategy, ConflictStrategy::Merge);
    assert!(
        pipeline::find_for_user(&t.db, eve.id, ids[0])
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        pipeline::find_item(&t.db, ids[0], 1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        pipeline::items_with_matches(&t.db, ids[0])
            .await
            .unwrap()
            .is_empty()
    );
}

// ------------------------------------------------------------------ parsers

#[test]
fn the_mal_xml_fixture_reads_every_field_mal_gives() {
    let rows = mal::parse_mal_xml(&fixture("mal-animelist.xml")).unwrap();
    assert_eq!(rows.len(), 6);
    let eva = &rows[0];
    assert_eq!(eva.external_ids.mal_id.as_deref(), Some("30"));
    assert_eq!(eva.title.as_deref(), Some("Shinseiki Evangelion"));
    assert_eq!(eva.kind, Some(MediaKind::Show));
    assert_eq!(eva.total_episodes, Some(26));
    assert_eq!(eva.status.as_deref(), Some("completed"));
    assert_eq!(eva.score, Some(100));
    assert_eq!(eva.progress, Some(26));
    assert_eq!(eva.rewatch_count, Some(1));
    assert!(!eva.rewatching);
    assert_eq!(eva.notes.as_deref(), Some("Watched the Rebuilds after."));
    assert_eq!(
        eva.started.map(|d| (d.date, d.precision)),
        Some((
            NaiveDate::from_ymd_opt(2024, 1, 5).unwrap(),
            DatePrecision::Day
        ))
    );
    let op = &rows[1];
    // MAL's 0 is "no score", and 0 episodes is "unknown".
    assert_eq!(op.score, None);
    assert_eq!(op.total_episodes, None);
    assert_eq!(op.notes, None);
    assert_eq!(op.started.map(|d| d.precision), Some(DatePrecision::Month));
    assert_eq!(op.completed, None, "0000-00-00 is no date");
    assert_eq!(
        rows[2].started.map(|d| d.precision),
        Some(DatePrecision::Year)
    );
    assert_eq!(rows[3].status.as_deref(), Some("dropped"));
    assert_eq!(rows[4].status.as_deref(), Some("planning"));
    assert_eq!(rows[5].kind, Some(MediaKind::Movie));
}

#[test]
fn the_mal_api_fixture_reads_every_field_mal_gives() {
    let rows = mal::parse_mal_json(&fixture("mal-animelist.json")).unwrap();
    assert_eq!(rows.len(), 4);
    let eva = &rows[0];
    assert_eq!(eva.title.as_deref(), Some("Neon Genesis Evangelion"));
    assert_eq!(eva.year, Some(1995));
    assert!(eva.rewatching);
    assert_eq!(eva.rewatch_count, Some(1));
    assert_eq!(eva.completed.map(|d| d.precision), Some(DatePrecision::Day));
    assert_eq!(
        rows[1].started.map(|d| d.precision),
        Some(DatePrecision::Month)
    );
    assert_eq!(rows[2].status.as_deref(), Some("paused"));
    // Several pages, or the bare `data` array, read the same.
    let doc: serde_json::Value = serde_json::from_slice(&fixture("mal-animelist.json")).unwrap();
    let pages = serde_json::json!([doc.clone(), doc.clone()]);
    assert_eq!(
        mal::parse_mal_json(pages.to_string().as_bytes())
            .unwrap()
            .len(),
        8
    );
    let bare = doc["data"].to_string();
    assert_eq!(mal::parse_mal_json(bare.as_bytes()).unwrap(), rows);
}

#[test]
fn each_tmdb_shape_reads() {
    let account = tmdb::parse_tmdb_json(&fixture("tmdb-account.json")).unwrap();
    assert_eq!(account.len(), 4, "a title on three lists is one row");
    let row = |tmdb_id: &str| {
        account
            .iter()
            .find(|r| r.external_ids.tmdb_id.as_deref() == Some(tmdb_id))
            .unwrap()
    };
    let fight_club = row("550");
    assert_eq!(fight_club.kind, Some(MediaKind::Movie));
    assert_eq!(fight_club.status.as_deref(), Some("completed"));
    assert_eq!(fight_club.score, Some(95));
    assert_eq!(fight_club.favourite, Some(true));
    assert_eq!(fight_club.year, Some(1999));
    let breaking_bad = row("1396");
    assert_eq!(breaking_bad.kind, Some(MediaKind::Show));
    assert_eq!(breaking_bad.score, Some(100), "v4's account_rating");
    assert_eq!(row("13").status.as_deref(), Some("planning"));
    assert_eq!(row("64010").kind, Some(MediaKind::Show));

    let page = tmdb::parse_tmdb_json(&fixture("tmdb-rated-page.json")).unwrap();
    assert_eq!(page.len(), 2);
    let rated = page.iter().find(|r| r.score.is_some()).unwrap();
    assert_eq!(
        (rated.status.as_deref(), rated.score),
        (Some("completed"), Some(80))
    );
    let unrated = page.iter().find(|r| r.score.is_none()).unwrap();
    assert_eq!(unrated.status.as_deref(), Some("planning"));

    let loose = tmdb::parse_tmdb_json(&fixture("tmdb-loose.json")).unwrap();
    assert_eq!(loose.len(), 3);
    assert_eq!(loose[1].kind, Some(MediaKind::Show));
}

// ------------------------------------------------------------------ matching

#[tokio::test]
async fn a_mal_export_matches_by_link_and_title_and_keeps_the_rest() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::MalXml,
        &fixture("mal-animelist.xml"),
        ConflictStrategy::Skip,
    )
    .await;
    let report = pipeline_for(&t.db, &mock)
        .run(import_id)
        .await
        .unwrap()
        .counts;
    assert_eq!(
        (
            report.total,
            report.applied,
            report.unmatched,
            report.failed,
            report.pending_retry
        ),
        (6, 4, 1, 1, 0)
    );
    let items = rows(&t.db, import_id).await;
    use ImportOutcome::*;
    assert_eq!(
        outcomes(&items),
        [Applied, Applied, Applied, Unmatched, Applied, Failed]
    );
    assert_accounted(&t.db, ada.id, import_id).await;
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imp.status, ImportStatus::Completed);

    // Evangelion: by AniList's link, with everything MAL said.
    assert_eq!(items[0].confidence, Some(1.0));
    let eva = entry_for(&t.db, ada.id, "anilist:show:30").await.unwrap();
    assert_eq!(eva.entry.status, EntryStatus::Completed);
    assert_eq!(eva.entry.score, Some(100));
    assert_eq!(
        eva.entry.notes.as_deref(),
        Some("Watched the Rebuilds after.")
    );
    let p = eva.progress.unwrap();
    assert_eq!(
        (p.run_count, p.current_run_episodes),
        (2, 26),
        "a rewatch is a run"
    );
    let library = LibraryService::new(t.db.clone());
    let runs = library.runs(ada.id, eva.media.id).await.unwrap();
    assert_eq!(runs[0].started_on, NaiveDate::from_ymd_opt(2024, 1, 5));
    assert_eq!(runs[0].completed_on, NaiveDate::from_ymd_opt(2024, 2, 11));
    assert_eq!(runs[1].started_on, None, "no date is invented");
    let plays = library.plays(ada.id, eva.media.id).await.unwrap();
    assert!(
        plays
            .iter()
            .all(|p| p.source == PlaySource::Import && p.import_id == Some(import_id))
    );

    // One Piece: watching, twelve episodes, a month-precision start.
    let op = entry_for(&t.db, ada.id, "anilist:show:21").await.unwrap();
    assert_eq!(op.entry.status, EntryStatus::Watching);
    assert_eq!(op.entry.score, None);
    assert_eq!(op.progress.unwrap().current_run_episodes, 12);
    let runs = library.runs(ada.id, op.media.id).await.unwrap();
    assert_eq!(runs[0].started_precision, Some(DatePrecision::Month));

    // FMA:B has no AniList link: the title search finds it. A search is a
    // judgement, not AniList's link, so its MAL id is not written to the
    // catalog every account matches through.
    assert!(items[2].confidence.unwrap() >= 0.88);
    let fmab = entry_for(&t.db, ada.id, "anilist:show:5114").await.unwrap();
    assert_eq!(fmab.progress.unwrap().current_run_episodes, 64);
    assert_eq!(
        catalog::find_by_external(&t.db, RefProvider::Mal, "5114")
            .await
            .unwrap()
            .map(|m| m.id),
        None
    );

    // Hellsing is two titles of the same name: unmatched, with both.
    let hellsing = &items[3];
    assert_eq!(hellsing.matched_media_id, None);
    let candidates: Vec<Candidate> =
        serde_json::from_value(hellsing.candidates.clone().unwrap()).unwrap();
    let refs: Vec<&str> = candidates.iter().map(|c| c.reference.as_str()).collect();
    assert_eq!(refs.len(), 2);
    assert!(refs.contains(&"anilist:show:270") && refs.contains(&"anilist:show:777"));

    // Cowboy Bebop is planned: an entry, no run, no plays.
    let bebop = entry_for(&t.db, ada.id, "anilist:show:1").await.unwrap();
    assert_eq!(bebop.entry.status, EntryStatus::Planning);
    assert_eq!(bebop.progress.unwrap().run_count, 0);

    // The provider refused the last row's search: failed, with the reason.
    assert!(items[5].error.as_deref().unwrap().contains("rejected"));

    // Every change is in the event log.
    let history = library.history(ada.id, None, None, 1000).await.unwrap();
    assert!(history.items.len() > 100);

    // Resolving Hellsing by hand applies it, for this account only.
    let (item, media) = pipeline_for(&t.db, &mock)
        .resolve_item(import_id, 4, &id("anilist:show:270"))
        .await
        .unwrap();
    assert_eq!(item.outcome, ImportOutcome::Applied);
    assert_eq!(item.confidence, Some(1.0));
    assert!(
        item.candidates.is_some(),
        "the candidates it was shown are kept"
    );
    let hellsing = entry_for(&t.db, ada.id, "anilist:show:270").await.unwrap();
    assert_eq!(hellsing.entry.status, EntryStatus::Dropped);
    assert_eq!(hellsing.entry.score, Some(60));
    assert_eq!(hellsing.media.id, media.id);
    assert_eq!(hellsing.progress.unwrap().current_run_episodes, 4);
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((imp.applied_count, imp.unmatched_count), (5, 0));
    assert_accounted(&t.db, ada.id, import_id).await;

    // A row resolved to the title it has is left as it is; a title that
    // does not exist, and a row that is not there, are refused.
    let pipe = pipeline_for(&t.db, &mock);
    let (again, _) = pipe
        .resolve_item(import_id, 4, &id("anilist:show:270"))
        .await
        .unwrap();
    assert_eq!(again, item);
    assert!(matches!(
        pipe.resolve_item(import_id, 6, &id("anilist:show:999"))
            .await,
        Err(ResolveError::TitleNotFound(_))
    ));
    assert!(matches!(
        pipe.resolve_item(import_id, 60, &id("anilist:show:270"))
            .await,
        Err(ResolveError::NoSuchRow)
    ));

    assert_eq!(
        catalog::find_by_external(&t.db, RefProvider::Mal, "270")
            .await
            .unwrap()
            .map(|m| m.id),
        None,
        "a hand resolution is not written to the shared catalog"
    );

    // The same file again, merging: every title is found by its MAL id now
    // (Hellsing and FMA:B by Ada's own earlier matches), and nothing is
    // logged twice.
    let plays_before = common::count(&t.db, "plays").await;
    let again = submit(
        &t.db,
        ada.id,
        ImportSource::MalXml,
        &fixture("mal-animelist.xml"),
        ConflictStrategy::Merge,
    )
    .await;
    let report = pipeline_for(&t.db, &mock).run(again).await.unwrap().counts;
    assert_eq!((report.applied, report.unmatched, report.failed), (5, 0, 1));
    assert_eq!(common::count(&t.db, "plays").await, plays_before);
    let again_rows = rows(&t.db, again).await;
    assert_eq!(again_rows[3].matched_media_id, Some(media.id));
    assert_eq!(again_rows[3].confidence, Some(1.0));
    assert_eq!(again_rows[2].confidence, items[2].confidence);

    // Bob imports the same file: Ada's choices are hers. Hellsing is
    // ambiguous for him as it was for her.
    let bob = common::user(&t.db).await;
    let his = submit(
        &t.db,
        bob.id,
        ImportSource::MalXml,
        &fixture("mal-animelist.xml"),
        ConflictStrategy::Skip,
    )
    .await;
    pipeline_for(&t.db, &mock).run(his).await.unwrap();
    let his_rows = rows(&t.db, his).await;
    assert_eq!(his_rows[3].outcome, ImportOutcome::Unmatched);
    assert!(entry_for(&t.db, bob.id, "anilist:show:270").await.is_none());
}

/// A MAL XML export of `(MAL id, title, episodes watched)` rows, watching,
/// with no episode count and no dates.
fn mal_rows(rows: &[(i64, &str, i32)]) -> Vec<u8> {
    let anime: String = rows
        .iter()
        .map(|(mal, title, watched)| {
            format!(
                "<anime><series_animedb_id>{mal}</series_animedb_id>\
                 <series_title><![CDATA[{title}]]></series_title>\
                 <series_type>TV</series_type><series_episodes>0</series_episodes>\
                 <my_watched_episodes>{watched}</my_watched_episodes>\
                 <my_start_date>0000-00-00</my_start_date>\
                 <my_finish_date>0000-00-00</my_finish_date>\
                 <my_score>0</my_score><my_status>Watching</my_status>\
                 <my_times_watched>0</my_times_watched>\
                 <my_rewatching>0</my_rewatching></anime>"
            )
        })
        .collect();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" ?><myanimelist>{anime}</myanimelist>")
        .into_bytes()
}

#[tokio::test]
async fn a_low_confidence_match_is_re_pointed_and_an_unmatched_row_skipped() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let library = LibraryService::new(t.db.clone());
    // Ada already has the 2003 series in her library, planned.
    let fma = stacks_core::media::MediaService::new(t.db.clone(), Arc::new(mock.clone()))
        .ensure_in_catalog(&id("anilist:show:121"))
        .await
        .unwrap();
    library
        .upsert(ada.id, fma.id, Default::default())
        .await
        .unwrap();

    // Neither MAL id is linked: both rows go to the title search.
    let file = mal_rows(&[
        (905114, "Fullmetal Alchemist: Brotherhood", 3),
        (900270, "Hellsing", 2),
    ]);
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::MalXml,
        &file,
        ConflictStrategy::Merge,
    )
    .await;
    let pipe = pipeline_for(&t.db, &mock);
    pipe.run(import_id).await.unwrap();
    let items = rows(&t.db, import_id).await;
    assert_eq!(
        outcomes(&items),
        [ImportOutcome::Applied, ImportOutcome::Unmatched]
    );

    // The episode count is unknown, so the match is under review: it keeps
    // what it was chosen from, posters included.
    let brotherhood = &items[0];
    let confidence = brotherhood.confidence.unwrap();
    assert!((matching::AUTO_MATCH..matching::REVIEW_BELOW).contains(&confidence));
    let kept = matching::stored_candidates(brotherhood.candidates.clone());
    assert_eq!(
        kept.iter()
            .map(|c| c.reference.as_str())
            .collect::<Vec<_>>(),
        ["anilist:show:5114", "anilist:show:121"]
    );
    assert_eq!(
        kept[0].poster_url.as_deref(),
        Some("https://img.example.test/anilist:show:5114.jpg")
    );
    let unmatched = matching::stored_candidates(items[1].candidates.clone());
    assert!(unmatched.iter().all(|c| c.poster_url.is_some()));
    let wrong = entry_for(&t.db, ada.id, "anilist:show:5114").await.unwrap();
    assert_eq!(wrong.progress.unwrap().current_run_episodes, 3);

    // Re-pointed at the 2003 series: the entry the row created goes to the
    // trash with its plays voided, and the row applies to the entry Ada had.
    let (moved, media) = pipe
        .resolve_item(import_id, 1, &id("anilist:show:121"))
        .await
        .unwrap();
    assert_eq!(media.id, fma.id);
    assert_eq!(
        (moved.outcome, moved.matched_media_id, moved.confidence),
        (ImportOutcome::Applied, Some(fma.id), Some(1.0))
    );
    assert_eq!(
        matching::stored_candidates(moved.candidates.clone()),
        kept,
        "it can be re-pointed again"
    );
    assert!(
        entry_for(&t.db, ada.id, "anilist:show:5114")
            .await
            .is_none()
    );
    let trash = library.trash(ada.id).await.unwrap();
    assert_eq!(
        trash.iter().map(|e| e.media.id).collect::<Vec<_>>(),
        [wrong.media.id]
    );
    let voided = library.plays(ada.id, wrong.media.id).await.unwrap();
    assert_eq!(voided.len(), 3);
    assert!(voided.iter().all(|p| p.voided_at.is_some()));
    let right = entry_for(&t.db, ada.id, "anilist:show:121").await.unwrap();
    assert_eq!(
        right.entry.id,
        library.get(ada.id, fma.id).await.unwrap().unwrap().entry.id
    );
    assert_eq!(right.progress.unwrap().current_run_episodes, 3);

    // Back again: the entry Ada had stays, without this row's plays, and
    // the trashed one comes back with the row's episodes logged afresh.
    pipe.resolve_item(import_id, 1, &id("anilist:show:5114"))
        .await
        .unwrap();
    let kept_entry = entry_for(&t.db, ada.id, "anilist:show:121").await.unwrap();
    assert_eq!(kept_entry.progress.unwrap().current_run_episodes, 0);
    let back = entry_for(&t.db, ada.id, "anilist:show:5114").await.unwrap();
    assert_eq!(back.entry.id, wrong.entry.id);
    assert_eq!(back.progress.unwrap().current_run_episodes, 3);
    assert!(library.trash(ada.id).await.unwrap().is_empty());

    // Hellsing is set aside: skipped, no title, its candidates kept, and
    // the counts move with it. Skipping it again changes nothing.
    let skipped = pipe.skip_item(import_id, 2).await.unwrap();
    assert_eq!(
        (
            skipped.outcome,
            skipped.matched_media_id,
            skipped.confidence
        ),
        (ImportOutcome::Skipped, None, None)
    );
    assert_eq!(
        matching::stored_candidates(skipped.candidates.clone()),
        unmatched
    );
    assert_eq!(pipe.skip_item(import_id, 2).await.unwrap(), skipped);
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (imp.applied_count, imp.skipped_count, imp.unmatched_count),
        (1, 1, 0)
    );
    assert_accounted(&t.db, ada.id, import_id).await;
    assert!(entry_for(&t.db, ada.id, "anilist:show:270").await.is_none());

    // A row with a title is not skipped; a skipped row can still be matched.
    assert!(matches!(
        pipe.skip_item(import_id, 1).await,
        Err(ResolveError::NotResolvable {
            outcome: ImportOutcome::Applied,
            ..
        })
    ));
    assert!(matches!(
        pipe.skip_item(import_id, 9).await,
        Err(ResolveError::NoSuchRow)
    ));
    let (matched, _) = pipe
        .resolve_item(import_id, 2, &id("anilist:show:270"))
        .await
        .unwrap();
    assert_eq!(matched.outcome, ImportOutcome::Applied);
    let hellsing = entry_for(&t.db, ada.id, "anilist:show:270").await.unwrap();
    assert_eq!(hellsing.progress.unwrap().current_run_episodes, 2);
    assert_accounted(&t.db, ada.id, import_id).await;
}

#[tokio::test]
async fn the_mal_api_export_matches_and_counts_a_rewatch_in_progress() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::MalJson,
        &fixture("mal-animelist.json"),
        ConflictStrategy::Skip,
    )
    .await;
    let report = pipeline_for(&t.db, &mock)
        .run(import_id)
        .await
        .unwrap()
        .counts;
    assert_eq!((report.total, report.applied, report.unmatched), (4, 4, 0));
    assert_accounted(&t.db, ada.id, import_id).await;
    // The API export gives the year, which tells the two Hellsings apart.
    let hellsing = entry_for(&t.db, ada.id, "anilist:show:270").await.unwrap();
    assert_eq!(hellsing.entry.status, EntryStatus::Dropped);
    assert!(entry_for(&t.db, ada.id, "anilist:show:777").await.is_none());
    // One completed rewatch and one under way: three runs.
    let eva = entry_for(&t.db, ada.id, "anilist:show:30").await.unwrap();
    assert_eq!(eva.progress.unwrap().run_count, 3);
    let fmab = entry_for(&t.db, ada.id, "anilist:show:5114").await.unwrap();
    assert_eq!(fmab.entry.status, EntryStatus::Paused);
    assert_eq!(fmab.progress.unwrap().current_run_episodes, 20);
}

#[tokio::test]
async fn each_tmdb_shape_imports() {
    let t = test_db!();
    let mock = Mock::scripted();
    for (file, applied, unmatched) in [
        ("tmdb-account.json", 4, 0),
        ("tmdb-rated-page.json", 2, 0),
        ("tmdb-loose.json", 2, 1),
    ] {
        let ada = common::user(&t.db).await;
        let import_id = submit(
            &t.db,
            ada.id,
            ImportSource::TmdbJson,
            &fixture(file),
            ConflictStrategy::Skip,
        )
        .await;
        let report = pipeline_for(&t.db, &mock)
            .run(import_id)
            .await
            .unwrap()
            .counts;
        assert_eq!(
            (report.applied, report.unmatched),
            (applied, unmatched),
            "{file}"
        );
        assert_accounted(&t.db, ada.id, import_id).await;
        let fight_club = entry_for(&t.db, ada.id, "tmdb:movie:550").await.unwrap();
        assert_eq!(fight_club.entry.status, EntryStatus::Completed, "{file}");
        // A rated movie is a watched one.
        assert_eq!(fight_club.progress.unwrap().play_count, 1, "{file}");
        if file == "tmdb-account.json" {
            assert!(fight_club.entry.favourite);
            assert_eq!(fight_club.entry.score, Some(95));
            let reply = entry_for(&t.db, ada.id, "tmdb:show:64010").await.unwrap();
            assert_eq!(reply.entry.status, EntryStatus::Planning);
        }
    }
}

// ------------------------------------------------------------------ strategies

#[tokio::test]
async fn the_conflict_strategies_treat_an_existing_entry_as_documented() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let file = br#"[{"tmdb_id": 550, "title": "Fight Club", "media_type": "movie",
                     "status": "Completed", "rating": 9}]"#;
    // Ada has Fight Club paused, unscored, with notes.
    let fight_club = stacks_core::media::MediaService::new(t.db.clone(), Arc::new(mock.clone()))
        .ensure_in_catalog(&id("tmdb:movie:550"))
        .await
        .unwrap();
    let library = LibraryService::new(t.db.clone());
    library
        .upsert(
            ada.id,
            fight_club.id,
            stacks_core::library::EntryPatch {
                status: Some(EntryStatus::Paused),
                notes: stacks_core::Patch::Value("mine".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let state = || async {
        library
            .get(ada.id, fight_club.id)
            .await
            .unwrap()
            .unwrap()
            .entry
    };

    let (db, user, mock) = (&t.db, ada.id, &mock);
    let run = |strategy| async move {
        let import_id = submit(db, user, ImportSource::TmdbJson, file, strategy).await;
        pipeline_for(db, mock).run(import_id).await.unwrap().counts
    };
    let skip = run(ConflictStrategy::Skip).await;
    assert_eq!(skip.skipped, 1);
    let e = state().await;
    assert_eq!((e.status, e.score), (EntryStatus::Paused, None));

    let merge = run(ConflictStrategy::Merge).await;
    assert_eq!(merge.applied, 1);
    let e = state().await;
    assert_eq!(
        (e.status, e.score, e.notes.as_deref()),
        (EntryStatus::Paused, Some(90), Some("mine")),
        "merge fills the score and keeps the status"
    );

    run(ConflictStrategy::Overwrite).await;
    let e = state().await;
    assert_eq!(
        (e.status, e.notes.as_deref()),
        (EntryStatus::Completed, Some("mine")),
        "overwrite takes the row's status and keeps what the row lacks"
    );
}

// ------------------------------------------------------------------ outages

/// AniList is down for the whole first run: every row waits, none is lost,
/// the job backs off, and once AniList is back the same job finishes it.
#[tokio::test]
async fn an_import_during_an_outage_completes_later_without_losing_rows() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::MalXml,
        &fixture("mal-animelist.xml"),
        ConflictStrategy::Skip,
    )
    .await;
    let runner = JobRunner::new(JobContext {
        db: t.db.clone(),
        metadata: Arc::new(mock.clone()),
    });

    mock.set_down(Some(unavailable()));
    assert_eq!(runner.run_once().await.unwrap(), 1);
    let items = rows(&t.db, import_id).await;
    assert_eq!(items.len(), 6, "every row is recorded");
    assert!(
        items
            .iter()
            .all(|r| r.outcome == ImportOutcome::PendingRetry)
    );
    assert!(items.iter().all(|r| r.error.is_some()));
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imp.status, ImportStatus::Pending);
    assert!(imp.error.as_deref().unwrap().contains("waiting"));
    assert_accounted(&t.db, ada.id, import_id).await;
    let queued = job::Entity::find()
        .filter(job::Column::JobType.eq(stacks_core::entity::enums::JobType::Import))
        .one(&t.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(queued.status, JobStatus::Pending, "the job is rescheduled");
    assert!(
        queued.run_at.to_utc() > chrono::Utc::now(),
        "with a backoff"
    );
    assert!(queued.max_attempts > 3);
    assert!(
        LibraryService::new(t.db.clone())
            .all(ada.id)
            .await
            .unwrap()
            .is_empty()
    );

    // The backoff passes; AniList is back.
    mock.set_down(None);
    t.db.execute_unprepared("UPDATE jobs SET run_at = now()")
        .await
        .unwrap();
    assert_eq!(runner.run_once().await.unwrap(), 1);
    use ImportOutcome::*;
    assert_eq!(
        outcomes(&rows(&t.db, import_id).await),
        [Applied, Applied, Applied, Unmatched, Applied, Failed]
    );
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imp.status, ImportStatus::Completed);
    assert_eq!(imp.error, None);
    assert_accounted(&t.db, ada.id, import_id).await;
    let done = job::Entity::find_by_id(queued.id)
        .one(&t.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.status, JobStatus::Completed);
}

/// TMDB rows name their title outright, so an outage only costs metadata:
/// the rows are applied against placeholders the refresh job fills later.
#[tokio::test]
async fn a_tmdb_import_during_an_outage_applies_against_placeholders() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    mock.set_down(Some(unavailable()));
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::TmdbJson,
        &fixture("tmdb-account.json"),
        ConflictStrategy::Skip,
    )
    .await;
    let report = pipeline_for(&t.db, &mock)
        .run(import_id)
        .await
        .unwrap()
        .counts;
    assert_eq!((report.applied, report.pending_retry), (4, 0));
    let fight_club = catalog::find_by_ref(&t.db, &id("tmdb:movie:550"))
        .await
        .unwrap()
        .unwrap();
    assert!(fight_club.refresh_error.is_some(), "a placeholder");
}

/// A TMDB row that does not say whether it is a movie or a show needs TMDB
/// to tell: while TMDB is down the row waits, it is not unmatched.
#[tokio::test]
async fn a_tmdb_row_without_a_kind_waits_out_an_outage() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let file = br#"{"page": 1, "results": [{"id": 550, "rating": 8}]}"#;
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::TmdbJson,
        file,
        ConflictStrategy::Skip,
    )
    .await;
    assert_eq!(rows(&t.db, import_id).await.len(), 0);
    mock.set_down(Some(unavailable()));
    let run = pipeline_for(&t.db, &mock).run(import_id).await.unwrap();
    assert_eq!(run.status, ImportStatus::Pending);
    assert_eq!((run.counts.pending_retry, run.counts.unmatched), (1, 0));
    let items = rows(&t.db, import_id).await;
    assert_eq!(items[0].raw["kind"], serde_json::Value::Null, "no kind");
    assert_eq!(outcomes(&items), [ImportOutcome::PendingRetry]);

    mock.set_down(None);
    let run = pipeline_for(&t.db, &mock).run(import_id).await.unwrap();
    assert_eq!(
        (run.status, run.counts.applied),
        (ImportStatus::Completed, 1)
    );
    assert!(entry_for(&t.db, ada.id, "tmdb:movie:550").await.is_some());
}

/// A completed row without a finish date leaves run 1's finish empty: no
/// date is invented for the first watch either.
#[tokio::test]
async fn a_completed_row_without_a_finish_date_leaves_it_empty() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let file = br#"{"data": [{
        "node": {"id": 30, "title": "Neon Genesis Evangelion", "media_type": "tv",
                 "num_episodes": 26},
        "list_status": {"status": "completed", "score": 0, "num_episodes_watched": 26,
                        "start_date": "2024-01-05"}
    }]}"#;
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::MalJson,
        file,
        ConflictStrategy::Skip,
    )
    .await;
    let report = pipeline_for(&t.db, &mock)
        .run(import_id)
        .await
        .unwrap()
        .counts;
    assert_eq!(report.applied, 1);
    let eva = entry_for(&t.db, ada.id, "anilist:show:30").await.unwrap();
    assert_eq!(eva.entry.status, EntryStatus::Completed);
    let runs = LibraryService::new(t.db.clone())
        .runs(ada.id, eva.media.id)
        .await
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].started_on, NaiveDate::from_ymd_opt(2024, 1, 5));
    assert_eq!(runs[0].completed_on, None, "no finish date is invented");
    assert_eq!(runs[0].completed_precision, None);
}

/// The import job of `import_id`.
async fn import_job(db: &sea_orm::DatabaseConnection) -> job::Model {
    job::Entity::find()
        .filter(job::Column::JobType.eq(stacks_core::entity::enums::JobType::Import))
        .one(db)
        .await
        .unwrap()
        .unwrap()
}

/// Under a rate limit the job stops asking at the first refusal, every row
/// after it waits, and a run that settled rows does not spend an attempt;
/// one that settled none does. When the attempts run out the import says
/// it failed, and retrying the job resumes it.
#[tokio::test]
async fn a_rate_limited_import_keeps_its_attempts_while_it_progresses() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::MalXml,
        &fixture("mal-animelist.xml"),
        ConflictStrategy::Skip,
    )
    .await;
    let runner = JobRunner::new(JobContext {
        db: t.db.clone(),
        metadata: Arc::new(mock.clone()),
    });

    // Enough for the first row (its MAL lookup and its details), then 429s.
    mock.script().budget = Some(2);
    assert_eq!(runner.run_once().await.unwrap(), 1);
    let items = rows(&t.db, import_id).await;
    use ImportOutcome::*;
    assert_eq!(
        outcomes(&items),
        [
            Applied,
            PendingRetry,
            PendingRetry,
            PendingRetry,
            PendingRetry,
            PendingRetry
        ]
    );
    assert_eq!(
        mock.script().calls,
        3,
        "nothing is asked after the first refusal"
    );
    assert_accounted(&t.db, ada.id, import_id).await;
    let queued = import_job(&t.db).await;
    assert_eq!(queued.status, JobStatus::Pending);
    assert_eq!(
        queued.attempts, 0,
        "a run that progressed is not an attempt"
    );
    assert!(queued.run_at.to_utc() > chrono::Utc::now());

    // Still refused: no progress, so that run is an attempt.
    t.db.execute_unprepared("UPDATE jobs SET run_at = now()")
        .await
        .unwrap();
    assert_eq!(runner.run_once().await.unwrap(), 1);
    assert_eq!(import_job(&t.db).await.attempts, 1);

    // The last attempt is spent without progress: the job fails, and so does
    // the import, rather than sit pending saying it will retry.
    t.db.execute_unprepared("UPDATE jobs SET run_at = now(), attempts = max_attempts - 1")
        .await
        .unwrap();
    assert_eq!(runner.run_once().await.unwrap(), 1);
    let failed = import_job(&t.db).await;
    assert_eq!(failed.status, JobStatus::Failed);
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imp.status, ImportStatus::Failed);
    assert!(imp.error.as_deref().unwrap().contains("gave up"));
    assert_accounted(&t.db, ada.id, import_id).await;

    // Retrying the job resumes the waiting rows.
    mock.script().budget = None;
    stacks_core::jobs::repo::retry_job(&t.db, failed.id)
        .await
        .unwrap();
    assert_eq!(runner.run_once().await.unwrap(), 1);
    assert_eq!(
        outcomes(&rows(&t.db, import_id).await),
        [Applied, Applied, Applied, Unmatched, Applied, Failed]
    );
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((imp.status, imp.error), (ImportStatus::Completed, None));
    assert_eq!(import_job(&t.db).await.status, JobStatus::Completed);
}

/// A stored file that no longer parses fails the import for good: the job
/// is done, and no "import completed" notification is queued for it.
#[tokio::test]
async fn an_import_whose_file_no_longer_parses_fails_without_a_completion_notice() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::TmdbJson,
        TMDB,
        ConflictStrategy::Skip,
    )
    .await;
    t.db.execute_unprepared("UPDATE imports SET original_file = 'not json'::bytea")
        .await
        .unwrap();
    let runner = JobRunner::new(JobContext {
        db: t.db.clone(),
        metadata: Arc::new(mock.clone()),
    });
    assert_eq!(runner.run_once().await.unwrap(), 1);
    let imp = pipeline::find_for_user(&t.db, ada.id, import_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imp.status, ImportStatus::Failed);
    assert!(imp.error.is_some());
    assert_eq!(import_job(&t.db).await.status, JobStatus::Completed);
    let notifications = job::Entity::find()
        .filter(job::Column::JobType.eq(stacks_core::entity::enums::JobType::Notification))
        .all(&t.db)
        .await
        .unwrap();
    assert!(notifications.is_empty(), "{notifications:?}");
}

/// A row resolved by hand while the import job is matching the same row is
/// applied once: the job, finding it settled, leaves it as the user left it.
#[tokio::test]
async fn a_row_resolved_while_the_job_works_on_it_is_applied_once() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let mock = Mock::scripted();
    let import_id = submit(
        &t.db,
        ada.id,
        ImportSource::MalXml,
        &fixture("mal-animelist.xml"),
        ConflictStrategy::Merge,
    )
    .await;
    // An outage first: every row waits, and so may be resolved by hand.
    mock.set_down(Some(unavailable()));
    pipeline_for(&t.db, &mock).run(import_id).await.unwrap();
    assert_eq!(
        rows(&t.db, import_id).await[0].outcome,
        ImportOutcome::PendingRetry
    );
    mock.set_down(None);

    // The job reaches Evangelion's MAL lookup and is held there.
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    mock.script().gate = Some((30, entered.clone(), release.clone()));
    let job = {
        let pipeline = pipeline_for(&t.db, &mock);
        tokio::spawn(async move { pipeline.run(import_id).await })
    };
    entered.notified().await;

    // Meanwhile Ada resolves the row by hand.
    let (resolved, eva) = pipeline_for(&t.db, &mock)
        .resolve_item(import_id, 1, &id("anilist:show:30"))
        .await
        .unwrap();
    assert_eq!(resolved.outcome, ImportOutcome::Applied);
    let library = LibraryService::new(t.db.clone());
    let plays = library.plays(ada.id, eva.id).await.unwrap().len();

    release.notify_one();
    job.await.unwrap().unwrap();
    let row = &rows(&t.db, import_id).await[0];
    assert_eq!(
        (row.outcome, row.confidence, row.updated_at),
        (ImportOutcome::Applied, Some(1.0), resolved.updated_at),
        "the job left the resolved row alone"
    );
    assert_eq!(
        library.plays(ada.id, eva.id).await.unwrap().len(),
        plays,
        "applied once"
    );
    assert_accounted(&t.db, ada.id, import_id).await;
}
