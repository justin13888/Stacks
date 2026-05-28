use std::collections::{BTreeMap, HashMap};

use cameo::{CameoClient, DetailProvider};
use chrono::Datelike;
use sea_orm::DatabaseConnection;

use crate::media::id::{MediaId, MediaKind};
use crate::media::mapping;
use crate::media::models::MediaDetails;

use super::models::UserStats;
use super::repo::{self, StatsError};

/// Cap on the number of completed titles whose details are fetched from cameo
/// for the genre breakdown and watch-time estimate. Bounds the cost of the
/// stats endpoint at the price of slightly under-counting very large libraries.
const GENRE_SAMPLE_CAP: usize = 250;

pub struct StatsService {
    pub db: DatabaseConnection,
}

impl StatsService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Compute the per-user stats payload.
    ///
    /// Yearly counts and the monthly heatmap come from the database alone;
    /// genre breakdown and the watch-time estimate are derived from cameo
    /// metadata for the user's most recent completed titles (capped by
    /// [`GENRE_SAMPLE_CAP`]).
    pub async fn get_user_stats(
        &self,
        cameo: &CameoClient,
        user_id: &str,
    ) -> Result<UserStats, StatsError> {
        let statuses = repo::watch_statuses(&self.db, user_id).await?;
        let history = repo::watch_history_events(&self.db, user_id).await?;

        let yearly_counts = compute_yearly_counts(&statuses);
        let monthly_heatmap = compute_monthly_heatmap(&history);

        // The completed sample drives both genre breakdown and watch-time.
        let sample_ids: Vec<MediaId> = statuses
            .iter()
            .filter(|s| s.status == "completed")
            .filter_map(|s| MediaId::parse(&s.cameo_media_id).ok())
            .take(GENRE_SAMPLE_CAP)
            .collect();
        let (genre_breakdown, total_watch_time_minutes) =
            genre_and_runtime(cameo, &sample_ids).await;

        Ok(UserStats {
            total_watch_time_minutes,
            genre_breakdown,
            yearly_counts,
            monthly_heatmap,
        })
    }
}

/// Group completed titles by the year in which they were finished.
fn compute_yearly_counts(statuses: &[crate::entity::watch_status::Model]) -> Vec<(i32, u32)> {
    let mut yearly: BTreeMap<i32, u32> = BTreeMap::new();
    for s in statuses {
        if s.status == "completed"
            && let Some(completed_at) = s.completed_at
        {
            *yearly.entry(completed_at.year()).or_default() += 1;
        }
    }
    yearly.into_iter().collect()
}

/// Group watch-history events by `YYYY-MM`.
fn compute_monthly_heatmap(history: &[crate::entity::watch_history::Model]) -> Vec<(String, u32)> {
    let mut monthly: BTreeMap<String, u32> = BTreeMap::new();
    for h in history {
        let month = h.timestamp.format("%Y-%m").to_string();
        *monthly.entry(month).or_default() += 1;
    }
    monthly.into_iter().collect()
}

/// Concurrently fetch detail for each id, then tally genres and sum estimated
/// runtimes. Ids cameo cannot resolve are dropped silently.
async fn genre_and_runtime(cameo: &CameoClient, ids: &[MediaId]) -> (Vec<(String, u32)>, i64) {
    let fetches = ids.iter().map(|id| async move {
        match id.kind {
            MediaKind::Movie => cameo
                .movie_details(id.numeric)
                .await
                .ok()
                .and_then(|d| mapping::movie_details(&d).ok()),
            MediaKind::Tv => cameo
                .tv_show_details(id.numeric)
                .await
                .ok()
                .and_then(|d| mapping::tv_details(&d).ok()),
        }
    });
    let details: Vec<MediaDetails> = futures::future::join_all(fetches)
        .await
        .into_iter()
        .flatten()
        .collect();

    let mut genres: HashMap<String, u32> = HashMap::new();
    let mut minutes: i64 = 0;
    for d in &details {
        for g in &d.media.genres {
            *genres.entry(g.clone()).or_default() += 1;
        }
        minutes += estimate_minutes(d);
    }

    // Most-frequent genres first; tie-break alphabetically for stable output.
    let mut breakdown: Vec<(String, u32)> = genres.into_iter().collect();
    breakdown.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (breakdown, minutes)
}

/// Estimate watch time for one completed title: `episodes * episode_runtime`
/// for TV, the single runtime for a movie.
fn estimate_minutes(d: &MediaDetails) -> i64 {
    match (d.number_of_episodes, d.runtime_minutes) {
        (Some(episodes), Some(runtime)) => i64::from(episodes) * i64::from(runtime),
        (None, Some(runtime)) => i64::from(runtime),
        _ => 0,
    }
}
