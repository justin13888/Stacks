use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStats {
    /// Minutes watched, summed over non-voided plays.
    pub total_watch_time_minutes: i64,
    /// Genres of completed titles: `(genre_name, count)`, most frequent first.
    pub genre_breakdown: Vec<(String, u32)>,
    /// Watch runs completed per calendar year: `(year, count)`.
    pub yearly_counts: Vec<(i32, u32)>,
    /// Plays per month (ISO `YYYY-MM` string): `(month, count)`.
    pub monthly_heatmap: Vec<(String, u32)>,
}
