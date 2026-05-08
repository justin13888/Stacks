use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStats {
    /// Total estimated watch time in minutes.
    pub total_watch_time_minutes: i64,
    /// Breakdown by genre: `(genre_name, count)`.
    pub genre_breakdown: Vec<(String, u32)>,
    /// Titles completed per calendar year: `(year, count)`.
    pub yearly_counts: Vec<(i32, u32)>,
    /// Activity heatmap per month (ISO `YYYY-MM` string): `(month, count)`.
    pub monthly_heatmap: Vec<(String, u32)>,
}
