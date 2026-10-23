//! The `metadata_refresh` job: keep catalog snapshots current.
//!
//! Each run refreshes a batch of stale snapshots through the
//! [`MetadataProvider`](crate::media::MetadataProvider). A failed refresh
//! records `refresh_error` / `refresh_failures` on the row and leaves every
//! stored field as it was: snapshot data is never deleted because a provider
//! is down. Titles that keep failing back off (see
//! [`catalog::stale_snapshots`]).
//!
//! For shows someone is watching, a refresh also looks for new seasons and
//! episodes, stores them, and queues a `new_episode` notification for each
//! watcher. Detection keeps its own state — `seasons.announced_through` and
//! `media.episodes_checked_at`, written only here — because user reads store
//! new seasons and episodes too, and would otherwise hide them from it.

use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{EntryStatus, JobType};
use crate::entity::{episode, library_entry, media, season};
use crate::jobs::handlers::notification::NotificationPayload;
use crate::jobs::repo as jobs_repo;
use crate::jobs::{JobContext, JobError};
use crate::media::catalog;
use crate::media::id::{EpisodeId, MediaId, MediaKind};
use crate::notifications::service::NotificationEvent;

/// Titles refreshed per run when the payload does not say.
pub const DEFAULT_BATCH: u64 = 50;

/// How long a snapshot counts as fresh.
pub const FRESH_FOR_SECS: i64 = 24 * 60 * 60;

/// At most this many episodes of one title are announced per refresh, so a
/// provider that suddenly lists a whole back catalogue cannot flood anyone.
const MAX_ANNOUNCED_PER_TITLE: usize = 20;

/// Attempts for each queued notification job.
const NOTIFICATION_ATTEMPTS: i32 = 3;

/// Payload for a `metadata_refresh` job. Every field is optional, so the
/// periodic job carries none.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataRefreshPayload {
    /// Refresh exactly these titles, however fresh. Empty: the stalest batch.
    #[serde(default)]
    pub media_ids: Vec<Uuid>,
    /// Batch size for the stalest-first selection.
    #[serde(default)]
    pub batch_size: Option<u64>,
}

/// What one run did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefreshReport {
    /// Snapshots refreshed from the provider.
    pub refreshed: u32,
    /// Snapshots whose refresh failed (their data kept).
    pub failed: u32,
    /// Episodes stored that the catalog did not have.
    pub new_episodes: u32,
    /// Notification jobs queued.
    pub notifications: u32,
}

/// Job entry point.
pub async fn handle(ctx: &JobContext, payload: MetadataRefreshPayload) -> Result<(), JobError> {
    let report = run(ctx, payload).await?;
    tracing::info!(
        refreshed = report.refreshed,
        failed = report.failed,
        new_episodes = report.new_episodes,
        notifications = report.notifications,
        "metadata refresh finished"
    );
    Ok(())
}

/// Refresh one batch. Only a database failure fails the run; a provider
/// failure is recorded on its title and the batch goes on.
pub async fn run(
    ctx: &JobContext,
    payload: MetadataRefreshPayload,
) -> Result<RefreshReport, JobError> {
    let rows = if payload.media_ids.is_empty() {
        catalog::stale_snapshots(
            &ctx.db,
            FRESH_FOR_SECS,
            payload.batch_size.unwrap_or(DEFAULT_BATCH).max(1),
        )
        .await?
    } else {
        media::Entity::find()
            .filter(media::Column::Id.is_in(payload.media_ids.clone()))
            .all(&ctx.db)
            .await?
    };
    let ids: Vec<Uuid> = rows.iter().map(|m| m.id).collect();
    let refs = catalog::primary_refs(&ctx.db, &ids).await?;

    let mut report = RefreshReport::default();
    for row in &rows {
        match refs.get(&row.id) {
            Some(id) => refresh_one(ctx, row, id, &mut report).await?,
            None => tracing::warn!(media_id = %row.id, "snapshot has no fetchable primary ref"),
        }
    }
    Ok(report)
}

async fn refresh_one(
    ctx: &JobContext,
    row: &media::Model,
    id: &MediaId,
    report: &mut RefreshReport,
) -> Result<(), JobError> {
    let title = match ctx.metadata.details(id).await {
        Ok(title) => title,
        Err(e) => {
            catalog::record_refresh_failure(&ctx.db, row.id, &e.to_string()).await?;
            report.failed += 1;
            return Ok(());
        }
    };
    let stored = catalog::upsert_snapshot(&ctx.db, &title.snapshot).await?;
    report.refreshed += 1;
    if id.kind != MediaKind::Show {
        return Ok(());
    }

    catalog::upsert_seasons(&ctx.db, stored.id, &title.seasons).await?;

    let watchers = watching_users(ctx, stored.id).await?;
    if watchers.is_empty() {
        // Nobody to tell. Forget the baseline, so a later watcher starts
        // from a fresh one instead of being sent the backlog since.
        if stored.episodes_checked_at.is_some() {
            catalog::set_episodes_checked(&ctx.db, stored.id, None).await?;
        }
        return Ok(());
    }

    // Detection reads only state this job writes (`announced_through`,
    // `episodes_checked_at`). The season episode counts are no use: a user
    // opening the show stores the new counts and episodes before the job
    // ever sees them.
    let first_scan = stored.episodes_checked_at.is_none();
    let seasons: HashMap<i32, season::Model> = catalog::seasons_of(&ctx.db, stored.id)
        .await?
        .into_iter()
        .map(|s| (s.number, s))
        .collect();
    let stored_episodes = episode::Entity::find()
        .filter(episode::Column::MediaId.eq(stored.id))
        .all(&ctx.db)
        .await?;
    let mut announce: Vec<EpisodeId> = Vec::new();
    // `(season, announced_through)` to record once the title is done.
    let mut advance: Vec<(i32, i32)> = Vec::new();
    for season in &title.seasons {
        let Ok(number) = i32::try_from(season.season_number) else {
            continue;
        };
        let listed = season.episode_count.and_then(|c| i32::try_from(c).ok());
        let stored_season = seasons.get(&number);
        let mark = stored_season.and_then(|s| s.announced_through);
        let in_season = |e: &&episode::Model| stored_season.is_some_and(|s| e.season_id == s.id);
        let due = match (mark, listed) {
            // A season detection has not dealt with yet.
            (None, _) => true,
            // More episodes listed than have aired and been dealt with: new
            // ones, or listed ones still waiting for their air date. Counted
            // rather than compared with the mark, because a season's episode
            // numbers need not start at 1.
            (Some(through), Some(listed)) => {
                let dealt = stored_episodes
                    .iter()
                    .filter(in_season)
                    .filter(|e| e.number <= through && has_aired(e))
                    .count();
                usize::try_from(listed).unwrap_or(0) > dealt
            }
            (Some(_), None) => false,
        };
        if !due {
            continue;
        }
        let details = match ctx.metadata.season(id, season.season_number).await {
            Ok(details) => details,
            // Already classified, counted and logged by the provider. The
            // mark is not moved, so the next refresh fetches the season
            // again — except on a baseline, which falls back to what is
            // already known (the listed count, or the highest stored
            // episode) so a later run cannot take this season's back
            // catalogue for new episodes.
            Err(_) => {
                if first_scan && mark.is_none() {
                    let known = stored_episodes
                        .iter()
                        .filter(in_season)
                        .map(|e| e.number)
                        .max();
                    if let Some(through) = listed.max(known) {
                        advance.push((number, through));
                    }
                }
                continue;
            }
        };
        let added =
            catalog::upsert_season_episodes(&ctx.db, stored.id, &details.season, &details.episodes)
                .await?;
        report.new_episodes += u32::try_from(added.len()).unwrap_or(u32::MAX);

        let aired: Vec<episode::Model> = catalog::episodes_of(&ctx.db, stored.id, number)
            .await?
            .into_iter()
            .filter(has_aired)
            .collect();
        // Nothing is announced on a title's first scan: everything would
        // look new. A season first seen later is new, all of it.
        if !first_scan {
            let from = mark.unwrap_or(0);
            announce.extend(
                aired.iter().filter(|e| e.number > from).map(|e| {
                    EpisodeId::new(id.provider, id.numeric, season.season_number, ep_no(e))
                }),
            );
        }
        // Recorded only now that the season's episodes are stored. Episodes
        // listed but not yet aired stay past the mark and are announced by
        // the refresh after they air.
        let through = aired.iter().map(|e| e.number).max().unwrap_or(0);
        advance.push((number, mark.map_or(through, |m| m.max(through))));
    }

    announce.truncate(MAX_ANNOUNCED_PER_TITLE);
    // The notifications and the marks that stop them repeating commit
    // together: a failure leaves both for the next refresh.
    let txn = ctx.db.begin().await?;
    for (number, through) in &advance {
        catalog::advance_announced_through(&txn, stored.id, *number, *through).await?;
    }
    catalog::set_episodes_checked(&txn, stored.id, Some(Utc::now().into())).await?;
    for episode in &announce {
        for user_id in &watchers {
            let payload = NotificationPayload {
                user_id: *user_id,
                event: NotificationEvent::NewEpisode {
                    cameo_media_id: id.to_string(),
                    episode_id: episode.to_string(),
                },
            };
            let payload = serde_json::to_value(&payload)
                .map_err(|e| JobError::ExecutionFailed(format!("notification payload: {e}")))?;
            jobs_repo::enqueue_job(
                &txn,
                jobs_repo::build_job(JobType::Notification, Some(payload), NOTIFICATION_ATTEMPTS),
            )
            .await?;
            report.notifications += 1;
        }
    }
    txn.commit().await?;
    Ok(())
}

/// Users with a live `watching` entry for the title.
async fn watching_users(ctx: &JobContext, media_id: Uuid) -> Result<Vec<Uuid>, JobError> {
    Ok(library_entry::Entity::find()
        .filter(library_entry::Column::MediaId.eq(media_id))
        .filter(library_entry::Column::Status.eq(EntryStatus::Watching))
        .filter(library_entry::Column::DeletedAt.is_null())
        .all(&ctx.db)
        .await?
        .into_iter()
        .map(|e| e.user_id)
        .collect())
}

/// Whether an episode has an air date that is not in the future. An episode
/// with no date is not announced: providers list placeholders that way.
fn has_aired(e: &episode::Model) -> bool {
    e.air_date.is_some_and(|d| d <= Utc::now().date_naive())
}

fn ep_no(e: &episode::Model) -> u32 {
    u32::try_from(e.number).unwrap_or_default()
}
