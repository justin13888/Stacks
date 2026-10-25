use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use sea_orm::DatabaseConnection;
use sea_orm::Iterable;
use uuid::Uuid;

use crate::entity::enums::{Category, EntryStatus};
use crate::entity::{media, play, watch_run};

use super::models::{Binge, RatedTitle, Streak, TOP_RATED, UserStats};
use super::repo::{self, StatsData};

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
    ) -> Result<UserStats, super::StatsError> {
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
    let completed_in_year: HashSet<Uuid> = runs
        .iter()
        .filter(|r| r.completed_on.is_some())
        .map(|r| r.entry_id)
        .collect();
    let rated = rated_titles(data, &completed_in_year, year);
    let episode_plays = plays.iter().filter(|p| p.episode_id.is_some()).count();
    UserStats {
        year,
        status_counts: status_counts(data),
        category_counts: category_counts(data),
        total_watch_time_minutes: watch_time(data, &plays),
        genre_breakdown: genre_breakdown(data, &completed_in_year, year),
        yearly_counts: yearly_completions(&runs),
        monthly_heatmap: monthly_plays(&plays),
        episode_plays: count(episode_plays),
        movie_plays: count(plays.len() - episode_plays),
        score_distribution: score_distribution(&rated),
        top_rated: top_rated(rated),
        longest_streak: longest_streak(&plays),
        longest_binge: longest_binge(data, &plays),
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The scored titles the score figures cover: every scored live entry, or
/// with a year, those with a run completed in it.
fn rated_titles(
    data: &StatsData,
    completed_in_year: &HashSet<Uuid>,
    year: Option<i32>,
) -> Vec<RatedTitle> {
    data.entries
        .iter()
        .filter(|(e, _)| year.is_none() || completed_in_year.contains(&e.id))
        .filter_map(|(e, m)| {
            Some(RatedTitle {
                media: m.clone(),
                score: e.score?,
            })
        })
        .collect()
}

/// `(score, titles)`, lowest score first.
fn score_distribution(rated: &[RatedTitle]) -> Vec<(i16, u32)> {
    let mut scores: BTreeMap<i16, u32> = BTreeMap::new();
    for r in rated {
        *scores.entry(r.score).or_default() += 1;
    }
    scores.into_iter().collect()
}

/// The [`TOP_RATED`] best scored: highest first, ties by title (ignoring
/// case), then by catalog id so the order is total.
fn top_rated(mut rated: Vec<RatedTitle>) -> Vec<RatedTitle> {
    rated.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| {
                a.media
                    .title
                    .to_lowercase()
                    .cmp(&b.media.title.to_lowercase())
            })
            .then_with(|| a.media.id.cmp(&b.media.id))
    });
    rated.truncate(TOP_RATED);
    rated
}

/// The day a play was watched on, in the offset it was stored with (the
/// same day `monthly_plays` counts it in); `None` when unknown.
fn play_day(p: &play::Model) -> Option<NaiveDate> {
    p.watched_at.map(|at| at.date_naive())
}

/// The longest run of consecutive days with a play; the earliest of equal
/// length.
fn longest_streak(plays: &[&play::Model]) -> Option<Streak> {
    let days: BTreeSet<NaiveDate> = plays.iter().filter_map(|p| play_day(p)).collect();
    let mut best: Option<Streak> = None;
    let mut current: Option<Streak> = None;
    for day in days {
        current = Some(match current {
            Some(s) if s.end.succ_opt() == Some(day) => Streak {
                days: s.days + 1,
                end: day,
                ..s
            },
            _ => Streak {
                days: 1,
                start: day,
                end: day,
            },
        });
        if let Some(c) = current
            && best.is_none_or(|b| c.days > b.days)
        {
            best = Some(c);
        }
    }
    best
}

/// The most plays of one title on one day; the earliest day of equal
/// count, then the title first by name.
fn longest_binge(data: &StatsData, plays: &[&play::Model]) -> Option<Binge> {
    let run_entry: HashMap<Uuid, Uuid> = data.runs.iter().map(|r| (r.id, r.entry_id)).collect();
    let entry_media: HashMap<Uuid, &media::Model> =
        data.entries.iter().map(|(e, m)| (e.id, m)).collect();
    let mut per_day: HashMap<(Uuid, NaiveDate), u32> = HashMap::new();
    for p in plays {
        if let (Some(day), Some(entry)) = (play_day(p), run_entry.get(&p.run_id)) {
            *per_day.entry((*entry, day)).or_default() += 1;
        }
    }
    per_day
        .into_iter()
        .filter_map(|((entry, date), plays)| {
            Some(Binge {
                media: (*entry_media.get(&entry)?).clone(),
                date,
                plays,
            })
        })
        .min_by(|a, b| {
            b.plays
                .cmp(&a.plays)
                .then(a.date.cmp(&b.date))
                .then_with(|| {
                    a.media
                        .title
                        .to_lowercase()
                        .cmp(&b.media.title.to_lowercase())
                })
                .then(a.media.id.cmp(&b.media.id))
        })
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
    completed_in_year: &HashSet<Uuid>,
    year: Option<i32>,
) -> Vec<(String, u32)> {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::entity::enums::{MediaKind, PlaySource};
    use crate::entity::library_entry;

    fn at(day: &str) -> sea_orm::prelude::DateTimeWithTimeZone {
        let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        Utc.from_utc_datetime(&date.and_hms_opt(20, 0, 0).unwrap())
            .fixed_offset()
    }

    fn title(name: &str, kind: MediaKind) -> media::Model {
        media::Model {
            id: Uuid::now_v7(),
            kind,
            format: None,
            category: Category::Movie,
            title: name.into(),
            original_title: None,
            romaji_title: None,
            alt_titles: serde_json::json!([]),
            overview: None,
            poster_url: None,
            backdrop_url: None,
            start_date: None,
            end_date: None,
            status: None,
            runtime_minutes: None,
            genres: Vec::new(),
            origin_countries: Vec::new(),
            original_language: None,
            season_count: None,
            episode_count: None,
            raw: None,
            fetched_at: None,
            refresh_error: None,
            refresh_failures: 0,
            episodes_checked_at: None,
            created_at: at("2020-01-01"),
            updated_at: at("2020-01-01"),
        }
    }

    /// A fixture library: each title with its score, the day its one run
    /// completed (if it did), and the days of its plays.
    struct Fixture {
        data: StatsData,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                data: StatsData {
                    entries: Vec::new(),
                    runs: Vec::new(),
                    plays: Vec::new(),
                    episodes: Vec::new(),
                },
            }
        }

        fn title(
            &mut self,
            name: &str,
            kind: MediaKind,
            score: Option<i16>,
            completed: Option<&str>,
            plays: &[&str],
        ) -> &mut Self {
            let media = title(name, kind);
            let entry = library_entry::Model {
                id: Uuid::now_v7(),
                user_id: Uuid::nil(),
                media_id: media.id,
                status: if completed.is_some() {
                    EntryStatus::Completed
                } else {
                    EntryStatus::Watching
                },
                score,
                favourite: false,
                notes: None,
                deleted_at: None,
                created_at: at("2020-01-01"),
                updated_at: at("2020-01-01"),
            };
            let run = watch_run::Model {
                id: Uuid::now_v7(),
                entry_id: entry.id,
                run_number: 1,
                started_on: None,
                started_precision: None,
                completed_on: completed.map(|d| at(d).date_naive()),
                completed_precision: None,
                created_at: at("2020-01-01"),
                updated_at: at("2020-01-01"),
            };
            for day in plays {
                self.data.plays.push(play::Model {
                    id: Uuid::now_v7(),
                    run_id: run.id,
                    episode_id: (kind == MediaKind::Show).then(Uuid::now_v7),
                    watched_at: Some(at(day)),
                    source: PlaySource::Manual,
                    import_id: None,
                    voided_at: None,
                    created_at: at(day),
                    updated_at: at(day),
                });
            }
            self.data.entries.push((entry, media));
            self.data.runs.push(run);
            self
        }
    }

    fn names(rated: &[RatedTitle]) -> Vec<(&str, i16)> {
        rated
            .iter()
            .map(|r| (r.media.title.as_str(), r.score))
            .collect()
    }

    #[test]
    fn a_year_in_review_counts_scores_streaks_binges_and_kinds() {
        let mut f = Fixture::new();
        f.title(
            "Frieren",
            MediaKind::Show,
            Some(95),
            Some("2025-03-02"),
            // Three days running in March, and four episodes on one of them.
            &[
                "2025-03-01",
                "2025-03-02",
                "2025-03-02",
                "2025-03-02",
                "2025-03-02",
                "2025-03-03",
            ],
        )
        .title(
            "Dune",
            MediaKind::Movie,
            Some(80),
            Some("2025-06-10"),
            &["2025-06-10"],
        )
        .title(
            "Arrival",
            MediaKind::Movie,
            Some(80),
            Some("2025-06-11"),
            &["2025-06-11", "2025-06-12", "2025-06-13"],
        )
        // Scored, but finished the year before.
        .title(
            "Alien",
            MediaKind::Movie,
            Some(100),
            Some("2024-12-31"),
            &["2024-12-31"],
        )
        // Unscored, and not finished.
        .title("Severance", MediaKind::Show, None, None, &["2025-08-01"]);

        let year = compute(&f.data, Some(2025));
        assert_eq!(year.episode_plays, 7);
        assert_eq!(year.movie_plays, 4);
        assert_eq!(year.score_distribution, [(80, 2), (95, 1)]);
        // Ties on score go by title.
        assert_eq!(
            names(&year.top_rated),
            [("Frieren", 95), ("Arrival", 80), ("Dune", 80)]
        );
        // March has three days running; June 10 to 13 has four.
        assert_eq!(
            year.longest_streak,
            Some(Streak {
                days: 4,
                start: NaiveDate::from_ymd_opt(2025, 6, 10).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 6, 13).unwrap(),
            })
        );
        let binge = year.longest_binge.unwrap();
        assert_eq!(
            (binge.media.title.as_str(), binge.date, binge.plays),
            ("Frieren", NaiveDate::from_ymd_opt(2025, 3, 2).unwrap(), 4)
        );

        // All time: every scored title, and December 31 joins no streak.
        let all = compute(&f.data, None);
        assert_eq!(all.score_distribution, [(80, 2), (95, 1), (100, 1)]);
        assert_eq!(names(&all.top_rated)[0], ("Alien", 100));
        assert_eq!(all.movie_plays, 5);
        assert_eq!(all.longest_streak.unwrap().days, 4);

        // A quiet year has none of it.
        let quiet = compute(&f.data, Some(1999));
        assert_eq!((quiet.episode_plays, quiet.movie_plays), (0, 0));
        assert!(quiet.score_distribution.is_empty() && quiet.top_rated.is_empty());
        assert_eq!(quiet.longest_streak, None);
        assert_eq!(quiet.longest_binge, None);
    }

    #[test]
    fn a_streak_of_equal_length_keeps_the_earliest_and_spans_a_new_year() {
        let mut f = Fixture::new();
        f.title(
            "Dune",
            MediaKind::Movie,
            None,
            None,
            &[
                "2024-12-30",
                "2024-12-31",
                "2025-01-01",
                "2025-05-01",
                "2025-05-02",
                "2025-05-03",
            ],
        );
        let all = compute(&f.data, None);
        assert_eq!(
            all.longest_streak,
            Some(Streak {
                days: 3,
                start: NaiveDate::from_ymd_opt(2024, 12, 30).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            })
        );
        // Each day is one play, so the binge is the earliest day.
        let binge = all.longest_binge.unwrap();
        assert_eq!(
            (binge.date, binge.plays),
            (NaiveDate::from_ymd_opt(2024, 12, 30).unwrap(), 1)
        );
        // Within 2025 the year's own days count, not the run into it.
        assert_eq!(compute(&f.data, Some(2025)).longest_streak.unwrap().days, 3);
        assert_eq!(
            compute(&f.data, Some(2025)).longest_streak.unwrap().start,
            NaiveDate::from_ymd_opt(2025, 5, 1).unwrap()
        );
    }

    #[test]
    fn top_rated_holds_at_most_its_bound() {
        let mut f = Fixture::new();
        for i in 0..(TOP_RATED + 3) {
            f.title(
                &format!("Title {i:02}"),
                MediaKind::Movie,
                Some(50),
                Some("2025-01-01"),
                &[],
            );
        }
        let all = compute(&f.data, None);
        assert_eq!(all.top_rated.len(), TOP_RATED);
        assert_eq!(all.score_distribution, [(50, (TOP_RATED + 3) as u32)]);
        assert_eq!(all.longest_binge, None);
    }
}
