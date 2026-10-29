//! New-episode detection by the `metadata_refresh` job: what it stores and
//! what it announces, to whom, for TMDB seasons and for an AniList show's
//! one synthesised season. Split out of `metadata_fallback.rs` to keep that
//! file within its length limit.

use super::*;

#[tokio::test]
async fn new_episodes_are_stored_and_announced_to_watchers() {
    let t = test_db!();
    let db = &t.db;
    let watcher = common::user(db).await;
    let planner = common::user(db).await;
    let mock = MockProvider::default();
    let id = MediaId::new(Provider::Tmdb, MediaKind::Show, 1399);
    mock.script()
        .seasons
        .insert(id.clone(), vec![vec![(1, days_ago(14)), (2, days_ago(7))]]);
    let row = service(db, &mock).ensure_in_catalog(&id).await.unwrap();
    let library = LibraryService::new(db.clone());
    for (user, status) in [
        (&watcher, EntryStatus::Watching),
        (&planner, EntryStatus::Planning),
    ] {
        library
            .upsert(
                user.id,
                row.id,
                EntryPatch {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let payload = MetadataRefreshPayload {
        media_ids: vec![row.id],
        batch_size: None,
    };

    // The first scan is the baseline: season 1's two episodes are stored,
    // nothing is announced.
    let report = metadata_refresh::run(&ctx(db, &mock), payload.clone())
        .await
        .unwrap();
    assert_eq!((report.new_episodes, report.notifications), (2, 0));

    // Episode 3 airs, episode 4 is announced for next week, and a second
    // season appears with one aired episode.
    mock.script().seasons.insert(
        id.clone(),
        vec![
            vec![
                (1, days_ago(14)),
                (2, days_ago(7)),
                (3, days_ago(0)),
                (4, days_ago(-7)),
            ],
            vec![(1, days_ago(1))],
        ],
    );
    let report = metadata_refresh::run(&ctx(db, &mock), payload.clone())
        .await
        .unwrap();
    assert_eq!(report.refreshed, 1);
    // S1E3, S1E4 and S2E1 are new rows.
    assert_eq!(report.new_episodes, 3);
    // Announced: S1E3 and S2E1, to the one watcher (not the planner, and
    // not the unaired S1E4 or the already-known S1E1-2).
    assert_eq!(report.notifications, 2);
    assert_eq!(
        announced(db, watcher.id).await,
        vec!["tmdb:show:1399:1:3", "tmdb:show:1399:2:1"]
    );
    assert_eq!(catalog::episodes_of(db, row.id, 1).await.unwrap().len(), 4);

    // Running again announces nothing new.
    let report = metadata_refresh::run(&ctx(db, &mock), payload.clone())
        .await
        .unwrap();
    assert_eq!(report.notifications, 0);

    // S1E4, listed before it aired, is announced once it has.
    mock.script().seasons.get_mut(&id).unwrap()[0][3].1 = days_ago(0);
    let report = metadata_refresh::run(&ctx(db, &mock), payload.clone())
        .await
        .unwrap();
    assert_eq!(report.notifications, 1);
    assert_eq!(
        announced(db, watcher.id).await,
        vec![
            "tmdb:show:1399:1:3",
            "tmdb:show:1399:1:4",
            "tmdb:show:1399:2:1"
        ]
    );
    let report = metadata_refresh::run(&ctx(db, &mock), payload)
        .await
        .unwrap();
    assert_eq!(report.notifications, 0);
}

/// The episode ids queued as `new_episode` notifications, sorted; every one
/// must be addressed to `user`.
async fn announced(db: &sea_orm::DatabaseConnection, user: uuid::Uuid) -> Vec<String> {
    let mut ids: Vec<String> = job::Entity::find()
        .filter(job::Column::JobType.eq(JobType::Notification))
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|j| j.payload)
        .map(|p| {
            assert_eq!(p["user_id"], user.to_string());
            p["event"]["new_episode"]["episode_id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    ids.sort();
    ids
}

/// A show with one watcher, its detection baselined by one refresh.
async fn watched_show(
    db: &sea_orm::DatabaseConnection,
    mock: &MockProvider,
    numeric: i32,
) -> (MediaId, media::Model, uuid::Uuid) {
    let id = MediaId::new(Provider::Tmdb, MediaKind::Show, numeric);
    let season = vec![(1, days_ago(14)), (2, days_ago(7))];
    let (row, watcher) = watched(db, mock, &id, season).await;
    (id, row, watcher)
}

/// `id`, holding the one `season`, with one watcher, its detection
/// baselined by one refresh.
async fn watched(
    db: &sea_orm::DatabaseConnection,
    mock: &MockProvider,
    id: &MediaId,
    season: Season,
) -> (media::Model, uuid::Uuid) {
    let watcher = common::user(db).await;
    mock.script().seasons.insert(id.clone(), vec![season]);
    let row = service(db, mock).ensure_in_catalog(id).await.unwrap();
    LibraryService::new(db.clone())
        .upsert(
            watcher.id,
            row.id,
            EntryPatch {
                status: Some(EntryStatus::Watching),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let report = metadata_refresh::run(
        &ctx(db, mock),
        MetadataRefreshPayload {
            media_ids: vec![row.id],
            batch_size: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(report.notifications, 0, "the baseline announces nothing");
    (row, watcher.id)
}

#[tokio::test]
async fn a_watcher_opening_the_show_does_not_hide_its_new_episode() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    let (id, row, watcher) = watched_show(db, &mock, 1400).await;

    // Episode 3 airs, and the watcher opens the show before the job runs:
    // detail, seasons and episodes all store what is new upstream and
    // refresh `fetched_at`.
    mock.script().seasons.get_mut(&id).unwrap()[0].push((3, days_ago(0)));
    let svc = service(db, &mock);
    svc.get_detail(watcher, &id).await.unwrap();
    svc.get_seasons(&id).await.unwrap();
    assert_eq!(svc.get_episodes(&id, 1).await.unwrap().episodes.len(), 3);
    assert_eq!(catalog::episodes_of(db, row.id, 1).await.unwrap().len(), 3);

    // A day later, the periodic batch (no ids) still picks the show up —
    // its snapshot is fresh, but detection is due — and announces S1E3.
    db.execute_unprepared(&format!(
        "UPDATE media SET episodes_checked_at = now() - interval '25 hours' WHERE id = '{}'",
        row.id
    ))
    .await
    .unwrap();
    let report = metadata_refresh::run(&ctx(db, &mock), MetadataRefreshPayload::default())
        .await
        .unwrap();
    assert_eq!(report.refreshed, 1);
    assert_eq!(announced(db, watcher).await, vec!["tmdb:show:1400:1:3"]);
}

#[tokio::test]
async fn a_failed_season_fetch_is_announced_by_the_next_refresh() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    let (id, row, watcher) = watched_show(db, &mock, 1401).await;
    let payload = MetadataRefreshPayload {
        media_ids: vec![row.id],
        batch_size: None,
    };

    // Episode 3 airs, but the season fetch fails: the new count is stored
    // with the title, and nothing is announced yet.
    mock.script().seasons.get_mut(&id).unwrap()[0].push((3, days_ago(0)));
    mock.script().seasons_down = true;
    let report = metadata_refresh::run(&ctx(db, &mock), payload.clone())
        .await
        .unwrap();
    assert_eq!((report.refreshed, report.notifications), (1, 0));
    let seasons = catalog::seasons_of(db, row.id).await.unwrap();
    assert_eq!(seasons[0].episode_count, Some(3));
    assert_eq!(
        seasons[0].announced_through,
        Some(2),
        "the mark did not move"
    );

    // The next refresh fetches the season again and announces it.
    mock.script().seasons_down = false;
    let report = metadata_refresh::run(&ctx(db, &mock), payload)
        .await
        .unwrap();
    assert_eq!(report.notifications, 1);
    assert_eq!(announced(db, watcher).await, vec!["tmdb:show:1401:1:3"]);
}

#[tokio::test]
async fn a_baseline_without_season_fetches_falls_back_to_the_listed_count() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    mock.script().seasons_down = true;
    let (id, row, watcher) = watched_show(db, &mock, 1402).await;
    let seasons = catalog::seasons_of(db, row.id).await.unwrap();
    assert_eq!(seasons[0].announced_through, Some(2));

    // Only the episode that aired after the baseline is announced, not the
    // back catalogue the baseline could not fetch.
    mock.script().seasons_down = false;
    mock.script().seasons.get_mut(&id).unwrap()[0].push((3, days_ago(0)));
    let report = metadata_refresh::run(
        &ctx(db, &mock),
        MetadataRefreshPayload {
            media_ids: vec![row.id],
            batch_size: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(report.notifications, 1);
    assert_eq!(announced(db, watcher).await, vec!["tmdb:show:1402:1:3"]);
}

/// Refresh just `row`.
async fn refresh(
    db: &sea_orm::DatabaseConnection,
    mock: &MockProvider,
    row: &media::Model,
) -> metadata_refresh::RefreshReport {
    let payload = MetadataRefreshPayload {
        media_ids: vec![row.id],
        batch_size: None,
    };
    metadata_refresh::run(&ctx(db, mock), payload)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_grown_anilist_episode_count_is_announced() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    // A synthesised season: numbered episodes, none dated.
    let id = MediaId::new(Provider::AniList, MediaKind::Show, 21);
    let (row, watcher) = watched(db, &mock, &id, vec![(1, None), (2, None)]).await;
    let seasons = catalog::seasons_of(db, row.id).await.unwrap();
    assert_eq!(seasons[0].announced_through, Some(2));
    assert_eq!(refresh(db, &mock, &row).await.notifications, 0);

    // AniList's count grows by two: both are announced, once.
    mock.script().seasons.get_mut(&id).unwrap()[0].extend([(3, None), (4, None)]);
    let report = refresh(db, &mock, &row).await;
    assert_eq!((report.new_episodes, report.notifications), (2, 2));
    assert_eq!(
        announced(db, watcher).await,
        vec!["anilist:show:21:1:3", "anilist:show:21:1:4"]
    );
    assert_eq!(refresh(db, &mock, &row).await.notifications, 0);
}

#[tokio::test]
async fn an_anilist_episode_listed_ahead_is_announced_once_it_airs() {
    let t = test_db!();
    let db = &t.db;
    let mock = MockProvider::default();
    // AniList knows the total: episode 3 airs next week and is dated, and
    // the rest of the cour is listed undated after it.
    let id = MediaId::new(Provider::AniList, MediaKind::Show, 22);
    let season = vec![(1, None), (2, None), (3, days_ago(-7)), (4, None)];
    let (row, watcher) = watched(db, &mock, &id, season).await;
    let seasons = catalog::seasons_of(db, row.id).await.unwrap();
    assert_eq!(seasons[0].announced_through, Some(2), "1-2 have aired");
    assert_eq!(refresh(db, &mock, &row).await.notifications, 0);

    // Episode 3 airs, and AniList dates episode 4 as the next.
    mock.script().seasons.get_mut(&id).unwrap()[0] =
        vec![(1, None), (2, None), (3, None), (4, days_ago(-7))];
    assert_eq!(refresh(db, &mock, &row).await.notifications, 1);
    assert_eq!(announced(db, watcher).await, vec!["anilist:show:22:1:3"]);
    assert_eq!(refresh(db, &mock, &row).await.notifications, 0);
}
