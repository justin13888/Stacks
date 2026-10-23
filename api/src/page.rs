//! One page of a collection, and the opaque cursor that continues it.
//!
//! Every collection answers with [`Page<T>`]: the items, and the cursor for
//! the next page (`null` on the last). A cursor is an opaque token, base64url
//! over the position the next page starts after; clients pass it back
//! unchanged. A cursor the server did not issue is a 400 rather than a
//! silent first page, because serving page one to a client that asked for
//! page seven hides its bug and duplicates its data.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use kynos::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

/// How many items a page holds when the request does not say.
pub const DEFAULT_LIMIT: u32 = 50;
/// The most items a page may hold.
pub const MAX_LIMIT: u32 = 200;

/// One page of a collection.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Pass as `cursor` to fetch the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
        }
    }
}

/// The `cursor` and `limit` every collection accepts.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct PageParams {
    /// The `next_cursor` of the previous page. Omit for the first page.
    pub cursor: Option<String>,
    /// At most this many items (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u32>,
}

impl PageParams {
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    /// The position the request continues after, or `None` for page one.
    pub fn after<K: DeserializeOwned>(&self) -> Result<Option<K>, InvalidCursor> {
        self.cursor.as_deref().map(decode).transpose()
    }
}

/// The request's `cursor` was not one this server issued.
#[derive(Debug, thiserror::Error, ApiError)]
#[error("the cursor is not one this server issued; restart from the first page")]
#[problem(base = crate::problem::BASE, status = 400, title = "Invalid cursor")]
pub struct InvalidCursor;

/// The opaque cursor for a position.
pub fn encode<K: Serialize>(position: &K) -> String {
    // A position is a small owned value (ids, timestamps, page numbers), all
    // of which serialize; an empty cursor would only restart the listing.
    let bytes = serde_json::to_vec(position).unwrap_or_default();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The position a cursor names.
pub fn decode<K: DeserializeOwned>(cursor: &str) -> Result<K, InvalidCursor> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| InvalidCursor)?;
    serde_json::from_slice(&bytes).map_err(|_| InvalidCursor)
}

/// Page a collection already held in memory and ordered newest first by a
/// UUIDv7 key (which is time-ordered), continuing after the cursor's id.
///
/// For the small per-user collections (tags, lists, sessions, tokens,
/// channels): the whole set is one indexed query, and the cursor is still a
/// stable keyset position rather than an offset, so an item added or removed
/// between pages neither repeats nor skips another.
pub fn by_id_desc<T>(
    mut items: Vec<T>,
    id: impl Fn(&T) -> Uuid,
    params: &PageParams,
) -> Result<Page<T>, InvalidCursor> {
    let after: Option<Uuid> = params.after()?;
    items.sort_by_key(|item| std::cmp::Reverse(id(item)));
    if let Some(after) = after {
        items.retain(|item| id(item) < after);
    }
    Ok(take(items, params.limit(), |last| encode(&id(last))))
}

/// The first `limit` of `items`, with the cursor after the last one kept when
/// more follow.
pub fn take<T>(mut items: Vec<T>, limit: u32, cursor_after: impl Fn(&T) -> String) -> Page<T> {
    let limit = limit as usize;
    let more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = if more {
        items.last().map(cursor_after)
    } else {
        None
    };
    Page::new(items, next_cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(cursor: Option<String>, limit: u32) -> PageParams {
        PageParams {
            cursor,
            limit: Some(limit),
        }
    }

    #[test]
    fn a_cursor_round_trips_its_position() {
        let id = Uuid::now_v7();
        let cursor = encode(&id);
        assert_eq!(decode::<Uuid>(&cursor).ok(), Some(id));
    }

    #[test]
    fn a_foreign_cursor_is_refused() {
        assert!(decode::<Uuid>("not base64 !!").is_err());
        assert!(decode::<Uuid>(&URL_SAFE_NO_PAD.encode("{}")).is_err());
        assert!(params(Some("garbage".into()), 10).after::<Uuid>().is_err());
    }

    #[test]
    fn keyset_pages_cover_every_item_once() {
        let ids: Vec<Uuid> = (0..7).map(|_| Uuid::now_v7()).collect();
        let mut seen = Vec::new();
        let mut cursor = None;
        loop {
            let page = by_id_desc(ids.clone(), |id| *id, &params(cursor, 3)).expect("valid");
            seen.extend(page.items);
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        let mut expected = ids;
        expected.reverse();
        assert_eq!(seen, expected);
    }

    #[test]
    fn the_limit_is_clamped() {
        assert_eq!(PageParams::default().limit(), DEFAULT_LIMIT);
        assert_eq!(params(None, 10_000).limit(), MAX_LIMIT);
    }
}
