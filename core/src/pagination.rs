//! Keyset pagination.
//!
//! A listing is read one window at a time, continuing strictly after the
//! position of the last item the caller saw, never at an offset: a row added
//! or removed between two reads then neither repeats nor skips another. A
//! query asks for one row more than it returns, and that extra row is how it
//! knows another window follows.
//!
//! The position itself (which columns, in which order) belongs to each
//! listing; what travels over the wire, and how a foreign cursor is refused,
//! belongs to the HTTP layer.

/// One window of a keyset-paginated listing.
#[derive(Debug, Clone, PartialEq)]
pub struct Slice<T> {
    pub items: Vec<T>,
    /// Whether more items follow the last one.
    pub more: bool,
}

impl<T> Slice<T> {
    /// The first `limit` of `items`, which a query fetched with a limit of
    /// `limit + 1`.
    pub fn from_overfetch(mut items: Vec<T>, limit: u64) -> Self {
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let more = items.len() > limit;
        items.truncate(limit);
        Self { items, more }
    }

    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            more: false,
        }
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Slice<U> {
        Slice {
            items: self.items.into_iter().map(f).collect(),
            more: self.more,
        }
    }
}

/// The row count to fetch for a window of `limit`.
pub fn overfetch(limit: u64) -> u64 {
    limit.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extra_row_means_more() {
        let s = Slice::from_overfetch(vec![1, 2, 3], 2);
        assert_eq!(s.items, vec![1, 2]);
        assert!(s.more);
        let s = Slice::from_overfetch(vec![1, 2], 2);
        assert_eq!(s.items, vec![1, 2]);
        assert!(!s.more);
        assert_eq!(overfetch(u64::MAX), u64::MAX);
    }
}
