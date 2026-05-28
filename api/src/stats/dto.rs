use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};
use stacks_core::stats::UserStats;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GenreCount {
    pub genre: String,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct YearCount {
    pub year: i32,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MonthCount {
    /// ISO `YYYY-MM`.
    pub month: String,
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StatsResponse {
    pub total_watch_time_minutes: i64,
    pub genre_breakdown: Vec<GenreCount>,
    pub yearly_counts: Vec<YearCount>,
    pub monthly_heatmap: Vec<MonthCount>,
}

impl From<UserStats> for StatsResponse {
    fn from(s: UserStats) -> Self {
        Self {
            total_watch_time_minutes: s.total_watch_time_minutes,
            genre_breakdown: s
                .genre_breakdown
                .into_iter()
                .map(|(genre, count)| GenreCount { genre, count })
                .collect(),
            yearly_counts: s
                .yearly_counts
                .into_iter()
                .map(|(year, count)| YearCount { year, count })
                .collect(),
            monthly_heatmap: s
                .monthly_heatmap
                .into_iter()
                .map(|(month, count)| MonthCount { month, count })
                .collect(),
        }
    }
}
