//! The library listing: filters, sort orders and keyset positions.
//!
//! Every sort order is total, because each ends on the entry id, so a
//! position names exactly one place in the listing and the next window starts
//! strictly after it.

use chrono::NaiveDate;
use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ConnectionTrait, DbBackend, DbErr, EntityTrait, Statement, Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{Category, EntryStatus, MediaKind};
use crate::entity::{library_entry, media};
use crate::pagination::{Slice, overfetch};

use super::service::EntryView;

/// Which entries a listing holds. Every filter narrows; none set means the
/// whole live library.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryFilter {
    pub status: Option<EntryStatus>,
    pub category: Option<Category>,
    /// Only favourites (`true`) or only the rest (`false`).
    pub favourite: Option<bool>,
    /// Only entries carrying this tag.
    pub tag: Option<Uuid>,
    /// A case-insensitive substring of the title, original title or romaji
    /// title.
    pub q: Option<String>,
    /// List the trash instead of the live library.
    pub deleted: bool,
}

/// The order of a listing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    /// Most recently changed first.
    #[default]
    Updated,
    /// Most recently added first.
    Added,
    /// By title, A to Z.
    Title,
    /// Highest score first, unscored last.
    Score,
    /// Furthest along the current run first: the episodes it has watched
    /// (a movie it has watched counts as one).
    Progress,
    /// The current run most recently started first; runs with no start
    /// date, and entries with no run, last.
    Started,
}

/// Where in a listing a window ends: the sort key of its last entry, then
/// that entry's id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPosition {
    Updated(DateTimeWithTimeZone, Uuid),
    Added(DateTimeWithTimeZone, Uuid),
    Title(String, Uuid),
    Score(Option<i16>, Uuid),
    Progress(i64, Uuid),
    Started(Option<NaiveDate>, Uuid),
}

impl EntryPosition {
    /// The position of `view` in a listing sorted by `sort`.
    pub fn of(sort: LibrarySort, view: &EntryView) -> Self {
        let entry = &view.entry;
        match sort {
            LibrarySort::Updated => Self::Updated(entry.updated_at, entry.id),
            LibrarySort::Added => Self::Added(entry.created_at, entry.id),
            LibrarySort::Title => Self::Title(view.media.title.clone(), entry.id),
            LibrarySort::Score => Self::Score(entry.score, entry.id),
            LibrarySort::Progress => Self::Progress(progress_key(view), entry.id),
            LibrarySort::Started => Self::Started(
                view.current_run.as_ref().and_then(|r| r.started_on),
                entry.id,
            ),
        }
    }

    /// The sort this position belongs to.
    pub fn sort(&self) -> LibrarySort {
        match self {
            Self::Updated(..) => LibrarySort::Updated,
            Self::Added(..) => LibrarySort::Added,
            Self::Title(..) => LibrarySort::Title,
            Self::Score(..) => LibrarySort::Score,
            Self::Progress(..) => LibrarySort::Progress,
            Self::Started(..) => LibrarySort::Started,
        }
    }
}

/// What the `progress` order sorts on, computed the way [`PROGRESS_KEY`]
/// computes it in SQL: the distinct episodes the current run has watched,
/// or for a movie, whether the current run has watched it.
fn progress_key(view: &EntryView) -> i64 {
    let Some(p) = &view.progress else {
        return 0;
    };
    match view.media.kind {
        MediaKind::Movie => p.current_run_plays.min(1),
        MediaKind::Show => p.current_run_episodes,
    }
}

/// The current run's live plays, as `pr.episodes` and `pr.plays`: the same
/// counts as the `entry_progress` view's `current_run_*` columns, for one
/// entry instead of every entry.
const CURRENT_RUN_PLAYS: &str = " LEFT JOIN LATERAL (\
     SELECT count(DISTINCT pl.episode_id) AS episodes, count(pl.id) AS plays \
     FROM watch_runs r JOIN plays pl ON pl.run_id = r.id \
     WHERE r.entry_id = e.id AND pl.voided_at IS NULL \
     AND r.run_number = (SELECT max(run_number) FROM watch_runs WHERE entry_id = e.id)\
     ) pr ON true";

/// The `progress` order's key over [`CURRENT_RUN_PLAYS`].
const PROGRESS_KEY: &str =
    "(CASE WHEN m.kind = 'movie' THEN least(pr.plays, 1) ELSE pr.episodes END)";

/// The current run's start date, as `cur.started_on`.
const CURRENT_RUN_START: &str = " LEFT JOIN LATERAL (\
     SELECT started_on FROM watch_runs WHERE entry_id = e.id \
     ORDER BY run_number DESC LIMIT 1\
     ) cur ON true";

/// `%`, `_` and the escape character itself, escaped for `LIKE … ESCAPE '\'`.
fn like_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// SQL text with its positional parameters, built up clause by clause.
struct Sql {
    text: String,
    values: Vec<Value>,
}

impl Sql {
    /// The placeholder for `value`.
    fn bind(&mut self, value: impl Into<Value>) -> String {
        self.values.push(value.into());
        format!("${}", self.values.len())
    }
}

/// One window of a user's entries, with their snapshots, continuing after
/// `after` (which must belong to `sort`; the caller checks).
pub async fn list_entries<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    filter: &LibraryFilter,
    sort: LibrarySort,
    after: Option<&EntryPosition>,
    limit: u64,
) -> Result<Slice<(library_entry::Model, media::Model)>, DbErr> {
    let mut sql = Sql {
        text: String::from("SELECT e.* FROM library_entries e JOIN media m ON m.id = e.media_id"),
        values: Vec::new(),
    };
    match sort {
        LibrarySort::Progress => sql.text.push_str(CURRENT_RUN_PLAYS),
        LibrarySort::Started => sql.text.push_str(CURRENT_RUN_START),
        _ => {}
    }
    sql.text.push_str(" WHERE ");
    let user = sql.bind(user_id);
    sql.text.push_str(&format!("e.user_id = {user}"));
    sql.text.push_str(if filter.deleted {
        " AND e.deleted_at IS NOT NULL"
    } else {
        " AND e.deleted_at IS NULL"
    });
    if let Some(status) = filter.status {
        let p = sql.bind(status.as_str());
        sql.text.push_str(&format!(" AND e.status = {p}"));
    }
    if let Some(category) = filter.category {
        let p = sql.bind(category.as_str());
        sql.text.push_str(&format!(" AND m.category = {p}"));
    }
    if let Some(favourite) = filter.favourite {
        let p = sql.bind(favourite);
        sql.text.push_str(&format!(" AND e.favourite = {p}"));
    }
    if let Some(tag) = filter.tag {
        let p = sql.bind(tag);
        sql.text.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM entry_tags t WHERE t.entry_id = e.id AND t.tag_id = {p})"
        ));
    }
    if let Some(q) = filter.q.as_deref().filter(|q| !q.is_empty()) {
        let p = sql.bind(like_pattern(q));
        sql.text.push_str(&format!(
            " AND (m.title ILIKE {p} ESCAPE '\\' OR m.original_title ILIKE {p} ESCAPE '\\' \
             OR m.romaji_title ILIKE {p} ESCAPE '\\')"
        ));
    }
    match after {
        None => {}
        Some(EntryPosition::Updated(at, id)) => {
            let (a, i) = (sql.bind(*at), sql.bind(*id));
            sql.text.push_str(&format!(
                " AND (e.updated_at, e.id) < ({a}::timestamptz, {i})"
            ));
        }
        Some(EntryPosition::Added(at, id)) => {
            let (a, i) = (sql.bind(*at), sql.bind(*id));
            sql.text.push_str(&format!(
                " AND (e.created_at, e.id) < ({a}::timestamptz, {i})"
            ));
        }
        Some(EntryPosition::Title(title, id)) => {
            let (t, i) = (sql.bind(title.clone()), sql.bind(*id));
            sql.text.push_str(&format!(
                " AND (lower(m.title), e.id) > (lower({t}::text), {i})"
            ));
        }
        Some(EntryPosition::Score(Some(score), id)) => {
            let (s, i) = (sql.bind(*score), sql.bind(*id));
            sql.text.push_str(&format!(
                " AND (e.score < {s} OR (e.score = {s} AND e.id < {i}) OR e.score IS NULL)"
            ));
        }
        Some(EntryPosition::Score(None, id)) => {
            let i = sql.bind(*id);
            sql.text
                .push_str(&format!(" AND e.score IS NULL AND e.id < {i}"));
        }
        Some(EntryPosition::Progress(watched, id)) => {
            let (w, i) = (sql.bind(*watched), sql.bind(*id));
            sql.text
                .push_str(&format!(" AND ({PROGRESS_KEY}, e.id) < ({w}::bigint, {i})"));
        }
        Some(EntryPosition::Started(Some(on), id)) => {
            let (d, i) = (sql.bind(*on), sql.bind(*id));
            sql.text.push_str(&format!(
                " AND (cur.started_on < {d}::date OR (cur.started_on = {d}::date AND e.id < {i}) \
                 OR cur.started_on IS NULL)"
            ));
        }
        Some(EntryPosition::Started(None, id)) => {
            let i = sql.bind(*id);
            sql.text
                .push_str(&format!(" AND cur.started_on IS NULL AND e.id < {i}"));
        }
    }
    let order = match sort {
        LibrarySort::Updated => " ORDER BY e.updated_at DESC, e.id DESC".to_owned(),
        LibrarySort::Added => " ORDER BY e.created_at DESC, e.id DESC".to_owned(),
        LibrarySort::Title => " ORDER BY lower(m.title) ASC, e.id ASC".to_owned(),
        LibrarySort::Score => " ORDER BY e.score DESC NULLS LAST, e.id DESC".to_owned(),
        LibrarySort::Progress => format!(" ORDER BY {PROGRESS_KEY} DESC, e.id DESC"),
        LibrarySort::Started => " ORDER BY cur.started_on DESC NULLS LAST, e.id DESC".to_owned(),
    };
    sql.text.push_str(&order);
    let fetch = i64::try_from(overfetch(limit)).unwrap_or(i64::MAX);
    let l = sql.bind(fetch);
    sql.text.push_str(&format!(" LIMIT {l}"));

    let entries = library_entry::Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql.text,
            sql.values,
        ))
        .all(db)
        .await?;
    let rows = super::repo::with_media(db, entries).await?;
    Ok(Slice::from_overfetch(rows, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_metacharacters_match_literally() {
        assert_eq!(like_pattern("50%_off\\"), "%50\\%\\_off\\\\%");
    }

    #[test]
    fn a_position_knows_its_sort() {
        let id = Uuid::now_v7();
        assert_eq!(EntryPosition::Score(None, id).sort(), LibrarySort::Score);
        assert_eq!(
            EntryPosition::Title("a".into(), id).sort(),
            LibrarySort::Title
        );
    }
}
