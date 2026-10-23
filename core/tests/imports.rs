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
use stacks_core::import_export::import::matching::Candidate;
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
        match &self.script().down {
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
                    tagline: None,
                    runtime_minutes: None,
                    status: None,
                    homepage: None,
                    imdb_id: None,
                    number_of_seasons: None,
                    number_of_episodes: None,
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
    let report = pipeline_for(&t.db, &mock).run(import_id).await.unwrap();
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

    // FMA:B has no AniList link: the title search finds it, and its MAL id
    // is recorded so the next import needs no search.
    assert!(items[2].confidence.unwrap() >= 0.88);
    let fmab = entry_for(&t.db, ada.id, "anilist:show:5114").await.unwrap();
    assert_eq!(fmab.progress.unwrap().current_run_episodes, 64);
    assert_eq!(
        catalog::find_by_external(&t.db, RefProvider::Mal, "5114")
            .await
            .unwrap()
            .map(|m| m.id),
        Some(fmab.media.id)
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

    // Resolving Hellsing by hand applies it and links its MAL id.
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

    // A row with its title already is not resolved again; nor is a title
    // that does not exist, nor a row that is not there.
    let pipe = pipeline_for(&t.db, &mock);
    assert!(matches!(
        pipe.resolve_item(import_id, 1, &id("anilist:show:270"))
            .await,
        Err(ResolveError::NotResolvable(ImportOutcome::Applied))
    ));
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

    // The same file again, merging: every title is found by its MAL id now
    // (Hellsing included), and nothing is logged twice.
    let plays_before = common::count(&t.db, "plays").await;
    let again = submit(
        &t.db,
        ada.id,
        ImportSource::MalXml,
        &fixture("mal-animelist.xml"),
        ConflictStrategy::Merge,
    )
    .await;
    let report = pipeline_for(&t.db, &mock).run(again).await.unwrap();
    assert_eq!((report.applied, report.unmatched, report.failed), (5, 0, 1));
    assert_eq!(common::count(&t.db, "plays").await, plays_before);
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
    let report = pipeline_for(&t.db, &mock).run(import_id).await.unwrap();
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
        let report = pipeline_for(&t.db, &mock).run(import_id).await.unwrap();
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
        pipeline_for(db, mock).run(import_id).await.unwrap()
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
    let report = pipeline_for(&t.db, &mock).run(import_id).await.unwrap();
    assert_eq!((report.applied, report.pending_retry), (4, 0));
    let fight_club = catalog::find_by_ref(&t.db, &id("tmdb:movie:550"))
        .await
        .unwrap()
        .unwrap();
    assert!(fight_club.refresh_error.is_some(), "a placeholder");
}
