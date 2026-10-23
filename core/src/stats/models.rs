use serde::{Deserialize, Serialize};

use crate::entity::enums::{Category, EntryStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserStats {
    /// The calendar year the activity figures are limited to, if any.
    pub year: Option<i32>,
    /// Live library entries per status, in status order, omitting zeros.
    /// The library as it stands: not limited by `year`.
    pub status_counts: Vec<(EntryStatus, u32)>,
    /// Live library entries per browsing category, omitting zeros. Not
    /// limited by `year`.
    pub category_counts: Vec<(Category, u32)>,
    /// Minutes watched, summed over non-voided plays (within `year`).
    pub total_watch_time_minutes: i64,
    /// Genres of completed titles (with a run completed within `year`):
    /// `(genre_name, count)`, most frequent first.
    pub genre_breakdown: Vec<(String, u32)>,
    /// Watch runs completed per calendar year: `(year, count)`.
    pub yearly_counts: Vec<(i32, u32)>,
    /// Plays per month (ISO `YYYY-MM` string): `(month, count)`.
    pub monthly_heatmap: Vec<(String, u32)>,
}
