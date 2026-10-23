//! The library listing: filters, sort orders and keyset positions.
//!
//! Every sort order is total, because each ends on the entry id, so a
//! position names exactly one place in the listing and the next window starts
//! strictly after it.

use sea_orm::prelude::DateTimeWithTimeZone;
use sea_orm::{ConnectionTrait, DbBackend, DbErr, EntityTrait, Statement, Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::enums::{Category, EntryStatus};
use crate::entity::{library_entry, media};
use crate::pagination::{Slice, overfetch};

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
}

impl EntryPosition {
    /// The position of `entry` in a listing sorted by `sort`.
    pub fn of(sort: LibrarySort, entry: &library_entry::Model, media: &media::Model) -> Self {
        match sort {
            LibrarySort::Updated => Self::Updated(entry.updated_at, entry.id),
            LibrarySort::Added => Self::Added(entry.created_at, entry.id),
            LibrarySort::Title => Self::Title(media.title.clone(), entry.id),
            LibrarySort::Score => Self::Score(entry.score, entry.id),
        }
    }

    /// The sort this position belongs to.
    pub fn sort(&self) -> LibrarySort {
        match self {
            Self::Updated(..) => LibrarySort::Updated,
            Self::Added(..) => LibrarySort::Added,
            Self::Title(..) => LibrarySort::Title,
            Self::Score(..) => LibrarySort::Score,
        }
    }
}

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
        text: String::from(
            "SELECT e.* FROM library_entries e JOIN media m ON m.id = e.media_id WHERE ",
        ),
        values: Vec::new(),
    };
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
    }
    sql.text.push_str(match sort {
        LibrarySort::Updated => " ORDER BY e.updated_at DESC, e.id DESC",
        LibrarySort::Added => " ORDER BY e.created_at DESC, e.id DESC",
        LibrarySort::Title => " ORDER BY lower(m.title) ASC, e.id ASC",
        LibrarySort::Score => " ORDER BY e.score DESC NULLS LAST, e.id DESC",
    });
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
