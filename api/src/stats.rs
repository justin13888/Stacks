//! The caller's statistics, computed from their library and watch history.

use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::stats::{StatsService, UserStats};

use crate::app::App;
use crate::catalog::{Category, Status};
use crate::problem::Internal;
use crate::security::{Caller, Reader};

/// The caller's statistics.
#[derive(Tag)]
#[tag(
    name = "stats",
    description = "Counts, watch time, activity and genres, from the caller's library and plays."
)]
pub struct StatsTag;

/// Entries in one status.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct StatusCount {
    pub status: Status,
    pub count: u32,
}

/// Entries in one browsing category.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct CategoryCount {
    pub category: Category,
    pub count: u32,
}

/// Watch runs completed in one calendar year.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct YearCount {
    pub year: i32,
    pub completed: u32,
}

/// Plays logged in one month.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct MonthCount {
    /// `YYYY-MM`.
    pub month: String,
    pub plays: u32,
}

/// Completed titles carrying one genre.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct GenreCount {
    pub genre: String,
    pub count: u32,
}

/// The caller's statistics.
///
/// `by_status` and `by_category` describe the library as it stands; the
/// rest is activity, limited to `year` when one was asked for.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Stats {
    /// The year the activity figures are limited to; `null` for all time.
    pub year: Option<i32>,
    /// Library entries per status, omitting statuses with none.
    pub by_status: Vec<StatusCount>,
    /// Library entries per category, omitting categories with none.
    pub by_category: Vec<CategoryCount>,
    /// Minutes watched, from each play's episode (or title) runtime.
    pub watch_time_minutes: i64,
    /// Watch runs completed per year, oldest first (a rewatch counts again).
    pub completions_by_year: Vec<YearCount>,
    /// Plays per month, oldest first.
    pub monthly_activity: Vec<MonthCount>,
    /// Genres of completed titles, most frequent first.
    pub top_genres: Vec<GenreCount>,
}

impl From<UserStats> for Stats {
    fn from(s: UserStats) -> Self {
        Self {
            year: s.year,
            by_status: s
                .status_counts
                .into_iter()
                .map(|(status, count)| StatusCount {
                    status: status.into(),
                    count,
                })
                .collect(),
            by_category: s
                .category_counts
                .into_iter()
                .map(|(category, count)| CategoryCount {
                    category: category.into(),
                    count,
                })
                .collect(),
            watch_time_minutes: s.total_watch_time_minutes,
            completions_by_year: s
                .yearly_counts
                .into_iter()
                .map(|(year, completed)| YearCount { year, completed })
                .collect(),
            monthly_activity: s
                .monthly_heatmap
                .into_iter()
                .map(|(month, plays)| MonthCount { month, plays })
                .collect(),
            top_genres: s
                .genre_breakdown
                .into_iter()
                .map(|(genre, count)| GenreCount { genre, count })
                .collect(),
        }
    }
}

/// How the statistics are limited.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct StatsParams {
    /// Limit the activity figures to this calendar year.
    #[schema(minimum = 1900, maximum = 3000)]
    pub year: Option<i32>,
}

/// What reading the statistics can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum StatsError {
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// Read the caller's statistics.
#[kynos::get("/stats", operation_id = "getStats")]
async fn get_stats(
    Auth(caller): Reader,
    Query(params): Query<StatsParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Stats>, StatsError> {
    let caller = Caller::from(caller);
    let stats = StatsService::new(db)
        .get_user_stats(caller.id(), params.year)
        .await
        .map_err(Internal::log)?;
    Ok(Json(stats.into()))
}

pub(crate) fn group() -> Group<App> {
    Group::new("/")
        .tag::<StatsTag>()
        .mount(kynos::routes![get_stats])
}
