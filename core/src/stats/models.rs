use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::entity::enums::{Category, EntryStatus};
use crate::entity::media;

/// How many titles [`UserStats::top_rated`] holds at most.
pub const TOP_RATED: usize = 10;

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
    /// Non-voided plays of episodes (within `year`).
    pub episode_plays: u32,
    /// Non-voided plays of movies (within `year`).
    pub movie_plays: u32,
    /// The scores of the rated titles: `(score, count)`, lowest score
    /// first, omitting scores no title has. Every scored live entry, or with
    /// a year, those with a run completed in it.
    pub score_distribution: Vec<(i16, u32)>,
    /// The best-scored of the same titles, at most [`TOP_RATED`]: highest
    /// score first, ties by title.
    pub top_rated: Vec<RatedTitle>,
    /// The longest run of consecutive days with a play (within `year`);
    /// `None` without a dated play.
    pub longest_streak: Option<Streak>,
    /// The most plays of one title on one day (within `year`); `None`
    /// without a dated play.
    pub longest_binge: Option<Binge>,
}

/// A title and the score the user gave it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatedTitle {
    pub media: media::Model,
    pub score: i16,
}

/// Consecutive days, each with at least one play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Streak {
    pub days: u32,
    /// The first day; the earliest such run when several are as long.
    pub start: NaiveDate,
    /// The last day, inclusive.
    pub end: NaiveDate,
}

/// The plays of one title on one day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binge {
    pub media: media::Model,
    /// The day; the earliest when several days are as long.
    pub date: NaiveDate,
    pub plays: u32,
}
