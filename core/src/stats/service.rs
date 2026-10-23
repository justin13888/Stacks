use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::Datelike;
use sea_orm::DatabaseConnection;
use sea_orm::Iterable;
use uuid::Uuid;

use crate::entity::enums::{Category, EntryStatus};
use crate::entity::{play, watch_run};

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

    /// The user's statistics, with the activity figures (watch time, genres,
    /// completions, monthly plays) limited to `year` when given.
    pub async fn get_user_stats(
        &self,
        user_id: Uuid,
        year: Option<i32>,
    ) -> Result<UserStats, StatsError> {
        let data = repo::load(&self.db, user_id).await?;
        Ok(compute(&data, year))
    }
}

/// The pure computation, separated so it can be tested without a database.
pub fn compute(data: &StatsData, year: Option<i32>) -> UserStats {
    let runs: Vec<&watch_run::Model> = data
        .runs
        .iter()
        .filter(|r| year.is_none_or(|y| r.completed_on.is_some_and(|d| d.year() == y)))
        .collect();
    let plays: Vec<&play::Model> = data
        .plays
        .iter()
        .filter(|p| year.is_none_or(|y| p.watched_at.is_some_and(|at| at.year() == y)))
        .collect();
    UserStats {
        year,
        status_counts: status_counts(data),
        category_counts: category_counts(data),
        total_watch_time_minutes: watch_time(data, &plays),
        genre_breakdown: genre_breakdown(data, &runs, year),
        yearly_counts: yearly_completions(&runs),
        monthly_heatmap: monthly_plays(&plays),
    }
}

fn status_counts(data: &StatsData) -> Vec<(EntryStatus, u32)> {
    EntryStatus::iter()
        .map(|s| {
            let n = data.entries.iter().filter(|(e, _)| e.status == s).count() as u32;
            (s, n)
        })
        .filter(|(_, n)| *n > 0)
        .collect()
}

fn category_counts(data: &StatsData) -> Vec<(Category, u32)> {
    Category::iter()
        .map(|c| {
            let n = data.entries.iter().filter(|(_, m)| m.category == c).count() as u32;
            (c, n)
        })
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// Runs completed per calendar year (a rewatch completion counts again).
fn yearly_completions(runs: &[&watch_run::Model]) -> Vec<(i32, u32)> {
    let mut yearly: BTreeMap<i32, u32> = BTreeMap::new();
    for run in runs {
        if let Some(done) = run.completed_on {
            *yearly.entry(done.year()).or_default() += 1;
        }
    }
    yearly.into_iter().collect()
}

/// Plays per `YYYY-MM` of when they were watched.
fn monthly_plays(plays: &[&play::Model]) -> Vec<(String, u32)> {
    let mut monthly: BTreeMap<String, u32> = BTreeMap::new();
    for p in plays {
        if let Some(at) = p.watched_at {
            *monthly.entry(at.format("%Y-%m").to_string()).or_default() += 1;
        }
    }
    monthly.into_iter().collect()
}

/// Genres of completed titles, most frequent first (ties alphabetical). With
/// a year, only titles with a run completed in it count.
fn genre_breakdown(
    data: &StatsData,
    runs: &[&watch_run::Model],
    year: Option<i32>,
) -> Vec<(String, u32)> {
    let completed_in_year: HashSet<Uuid> = runs
        .iter()
        .filter(|r| r.completed_on.is_some())
        .map(|r| r.entry_id)
        .collect();
    let mut genres: HashMap<String, u32> = HashMap::new();
    for (entry, media) in &data.entries {
        let counts = match year {
            None => entry.status == EntryStatus::Completed,
            Some(_) => completed_in_year.contains(&entry.id),
        };
        if counts {
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
fn watch_time(data: &StatsData, plays: &[&play::Model]) -> i64 {
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
    plays
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
