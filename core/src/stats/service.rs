use std::collections::{BTreeMap, HashMap};

use chrono::Datelike;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::entity::enums::EntryStatus;

use super::models::UserStats;
use super::repo::{self, StatsData, StatsError};

/// Per-user statistics, computed entirely from the database: completions
/// from runs, activity and watch time from non-voided plays, genres from the
/// catalog snapshot. No provider is called, so stats work offline.
pub struct StatsService {
    db: DatabaseConnection,
}

impl StatsService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get_user_stats(&self, user_id: Uuid) -> Result<UserStats, StatsError> {
        let data = repo::load(&self.db, user_id).await?;
        Ok(compute(&data))
    }
}

/// The pure computation, separated so it can be tested without a database.
pub fn compute(data: &StatsData) -> UserStats {
    UserStats {
        total_watch_time_minutes: watch_time(data),
        genre_breakdown: genre_breakdown(data),
        yearly_counts: yearly_completions(data),
        monthly_heatmap: monthly_plays(data),
    }
}

/// Runs completed per calendar year (a rewatch completion counts again).
fn yearly_completions(data: &StatsData) -> Vec<(i32, u32)> {
    let mut yearly: BTreeMap<i32, u32> = BTreeMap::new();
    for run in &data.runs {
        if let Some(done) = run.completed_on {
            *yearly.entry(done.year()).or_default() += 1;
        }
    }
    yearly.into_iter().collect()
}

/// Plays per `YYYY-MM` of when they were watched.
fn monthly_plays(data: &StatsData) -> Vec<(String, u32)> {
    let mut monthly: BTreeMap<String, u32> = BTreeMap::new();
    for p in &data.plays {
        if let Some(at) = p.watched_at {
            *monthly.entry(at.format("%Y-%m").to_string()).or_default() += 1;
        }
    }
    monthly.into_iter().collect()
}

/// Genres of completed titles, most frequent first (ties alphabetical).
fn genre_breakdown(data: &StatsData) -> Vec<(String, u32)> {
    let mut genres: HashMap<String, u32> = HashMap::new();
    for (entry, media) in &data.entries {
        if entry.status == EntryStatus::Completed {
            for g in &media.genres {
                *genres.entry(g.clone()).or_default() += 1;
            }
        }
    }
    let mut breakdown: Vec<(String, u32)> = genres.into_iter().collect();
    breakdown.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    breakdown
}

/// Minutes watched: each play counts its episode's runtime, falling back to
/// the title's runtime (a movie's length, or a show's typical episode).
fn watch_time(data: &StatsData) -> i64 {
    let run_entry: HashMap<Uuid, Uuid> = data.runs.iter().map(|r| (r.id, r.entry_id)).collect();
    let entry_runtime: HashMap<Uuid, i32> = data
        .entries
        .iter()
        .filter_map(|(e, m)| Some((e.id, m.runtime_minutes?)))
        .collect();
    let episode_runtime: HashMap<Uuid, i32> = data
        .episodes
        .iter()
        .filter_map(|e| Some((e.id, e.runtime_minutes?)))
        .collect();
    data.plays
        .iter()
        .map(|p| {
            p.episode_id
                .and_then(|e| episode_runtime.get(&e).copied())
                .or_else(|| {
                    run_entry
                        .get(&p.run_id)
                        .and_then(|e| entry_runtime.get(e).copied())
                })
                .map_or(0, i64::from)
        })
        .sum()
}
