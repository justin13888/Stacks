use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct CursorParams {
    pub cursor: Option<String>,
    pub limit: Option<u64>,
}

impl CursorParams {
    pub fn limit_or_default(&self) -> u64 {
        self.limit.unwrap_or(50).min(200)
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> PaginatedResult<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }
}

// ── Page ⇄ cursor bridge ─────────────────────────────────────────────────────
//
// cameo (and the upstream TMDB API) paginate by 1-indexed page number, while
// the Stacks API exposes opaque cursors. These helpers translate between the
// two: the cursor simply carries the next page number, base64url-encoded so
// clients treat it as an opaque token.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Decode an opaque cursor into a 1-indexed page number for a cameo call.
///
/// An absent cursor means "first page". A malformed cursor also falls back to
/// page 1 rather than erroring: cursors are opaque server-issued tokens, so the
/// only way to see a bad one is tampering or a client bug, and serving the
/// first page is a safe, non-failing response for a read-only listing.
pub fn cursor_to_page(cursor: Option<&str>) -> u32 {
    cursor
        .and_then(|c| URL_SAFE_NO_PAD.decode(c).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&page| page >= 1)
        .unwrap_or(1)
}

/// Build the opaque cursor for the page *after* `current_page`, given the total
/// page count. Returns `None` when `current_page` is the last page.
pub fn next_page_cursor(current_page: u32, total_pages: u32) -> Option<String> {
    (current_page < total_pages).then(|| URL_SAFE_NO_PAD.encode((current_page + 1).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_defaults_to_first_page() {
        assert_eq!(cursor_to_page(None), 1);
        assert_eq!(cursor_to_page(Some("not base64 !!")), 1);
        assert_eq!(cursor_to_page(Some("")), 1);
    }

    #[test]
    fn cursor_round_trips() {
        let c1 = next_page_cursor(1, 5).expect("has next");
        assert_eq!(cursor_to_page(Some(&c1)), 2);
        let c2 = next_page_cursor(2, 5).expect("has next");
        assert_eq!(cursor_to_page(Some(&c2)), 3);
    }

    #[test]
    fn no_cursor_on_last_page() {
        assert_eq!(next_page_cursor(5, 5), None);
        assert_eq!(next_page_cursor(6, 5), None);
        assert_eq!(next_page_cursor(1, 1), None);
    }
}
