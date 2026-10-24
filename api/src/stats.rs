//! The caller's statistics, computed from their library and watch history.

use chrono::NaiveDate;
use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::stats::{StatsService, UserStats};
use uuid::Uuid;

use crate::app::App;
use crate::catalog::{Category, Status, TitleSummary, snapshot_summaries};
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

/// Rated titles given one score.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct ScoreCount {
    /// 0–100.
    pub score: u8,
    pub count: u32,
}

/// A title and the caller's score for it.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct RatedTitle {
    pub title: TitleSummary,
    /// 0–100.
    pub score: u8,
}

/// Consecutive days, each with at least one play.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Streak {
    pub days: u32,
    /// The first day. Of several streaks as long, the earliest.
    pub start: NaiveDate,
    /// The last day, inclusive.
    pub end: NaiveDate,
}

/// The plays of one title on one day.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Binge {
    pub title: TitleSummary,
    /// The day. Of several days as long, the earliest.
    pub date: NaiveDate,
    pub plays: u32,
}

/// The caller's statistics.
///
/// `by_status` and `by_category` describe the library as it stands; the
/// rest is activity, limited to `year` when one was asked for. Days are
/// the days plays were logged on, in UTC.
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
    /// Plays of episodes. A play whose date is unknown counts only for all
    /// time (and never in `monthly_activity`).
    pub episode_plays: u32,
    /// Plays of movies, counted as `episode_plays` is.
    pub movie_plays: u32,
    /// How many rated titles have each score, lowest score first, omitting
    /// scores none has: every scored title, or with `year`, those with a
    /// watch run completed in it.
    pub score_distribution: Vec<ScoreCount>,
    /// The best-scored of the same titles, at most 10: highest first, ties
    /// by title.
    pub top_rated: Vec<RatedTitle>,
    /// The longest run of consecutive days with a play; `null` without a
    /// dated play.
    pub longest_streak: Option<Streak>,
    /// The most plays of one title on one day; `null` without a dated play.
    pub longest_binge: Option<Binge>,
}

/// The score as the API states it: 0–100.
fn score(s: i16) -> u8 {
    u8::try_from(s).unwrap_or_default()
}

impl Stats {
    /// `s` rendered, its titles named from their catalog snapshots. A title
    /// that cannot be named (no public reference) is left out.
    async fn render(
        db: &DatabaseConnection,
        user_id: Uuid,
        s: UserStats,
    ) -> Result<Self, Internal> {
        let rows: Vec<_> = s
            .top_rated
            .iter()
            .map(|r| r.media.clone())
            .chain(s.longest_binge.iter().map(|b| b.media.clone()))
            .collect();
        let summaries = snapshot_summaries(db, user_id, &rows).await?;
        let top_rated = s
            .top_rated
            .iter()
            .filter_map(|r| {
                Some(RatedTitle {
                    title: summaries.get(&r.media.id)?.clone(),
                    score: score(r.score),
                })
            })
            .collect();
        let longest_binge = s.longest_binge.as_ref().and_then(|b| {
            Some(Binge {
                title: summaries.get(&b.media.id)?.clone(),
                date: b.date,
                plays: b.plays,
            })
        });
        Ok(Self {
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
            episode_plays: s.episode_plays,
            movie_plays: s.movie_plays,
            score_distribution: s
                .score_distribution
                .into_iter()
                .map(|(value, count)| ScoreCount {
                    score: score(value),
                    count,
                })
                .collect(),
            top_rated,
            longest_streak: s.longest_streak.map(|st| Streak {
                days: st.days,
                start: st.start,
                end: st.end,
            }),
            longest_binge,
        })
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
    let stats = StatsService::new(db.clone())
        .get_user_stats(caller.id(), params.year)
        .await
        .map_err(Internal::log)?;
    Ok(Json(Stats::render(&db, caller.id(), stats).await?))
}

pub(crate) fn group() -> Group<App> {
    Group::new("/")
        .tag::<StatsTag>()
        .mount(kynos::routes![get_stats])
}
