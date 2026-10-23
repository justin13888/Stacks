//! String fractional indexing for list order.
//!
//! A position is a non-empty string of base-62 digits (`0-9A-Za-z`, in ASCII
//! order) that never ends in `0`, read as the fractional part of a number in
//! `(0, 1)`. Strings compare bytewise exactly as those numbers compare, so
//! the `list_items.position` column (collation `"C"`) sorts correctly, and
//! there is always room for another key between any two: the float scheme
//! this replaces ran out of precision after ~50 inserts at one spot.

use thiserror::Error;

const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE: usize = DIGITS.len();

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FractionalIndexError {
    #[error("invalid position key: {0:?}")]
    InvalidKey(String),
    #[error("position keys out of order: {0:?} >= {1:?}")]
    OutOfOrder(String, String),
}

fn digit_value(b: u8) -> Option<usize> {
    DIGITS.iter().position(|d| *d == b)
}

/// Whether `key` is a well-formed position.
pub fn is_valid(key: &str) -> bool {
    !key.is_empty() && !key.ends_with('0') && key.bytes().all(|b| digit_value(b).is_some())
}

/// A key strictly between `before` and `after`. `None` stands for the start
/// (before) or end (after) of the list.
pub fn between(before: Option<&str>, after: Option<&str>) -> Result<String, FractionalIndexError> {
    for k in [before, after].into_iter().flatten() {
        if !is_valid(k) {
            return Err(FractionalIndexError::InvalidKey(k.to_owned()));
        }
    }
    if let (Some(a), Some(b)) = (before, after)
        && a >= b
    {
        return Err(FractionalIndexError::OutOfOrder(a.to_owned(), b.to_owned()));
    }
    Ok(midpoint(
        before.unwrap_or("").as_bytes(),
        after.map(str::as_bytes),
    ))
}

/// `n` evenly spread, increasing keys — used to lay out a whole list afresh.
pub fn spread(n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n);
    let mut prev: Option<String> = None;
    for _ in 0..n {
        let next = midpoint(prev.as_deref().unwrap_or("").as_bytes(), None);
        prev = Some(next.clone());
        out.push(next);
    }
    out
}

/// Midpoint of the fractions `a` and `b` (`b = None` meaning 1). Requires
/// `a < b`, neither ending in `0`.
fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        // Skip the shared prefix (with `a` padded by zeros).
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(b'0') == b[n] {
            n += 1;
        }
        if n > 0 {
            let prefix = String::from_utf8_lossy(&b[..n]).into_owned();
            let rest_a = if n < a.len() { &a[n..] } else { &[][..] };
            return prefix + &midpoint(rest_a, Some(&b[n..]));
        }
    }
    let da = a.first().and_then(|d| digit_value(*d)).unwrap_or(0);
    let db = b
        .and_then(|b| b.first())
        .and_then(|d| digit_value(*d))
        .unwrap_or(BASE);
    if db - da > 1 {
        let mid = (da + db).div_ceil(2);
        return (DIGITS[mid] as char).to_string();
    }
    // Adjacent first digits.
    if let Some(b) = b
        && b.len() > 1
    {
        return (b[0] as char).to_string();
    }
    let rest_a = if a.len() > 1 { &a[1..] } else { &[][..] };
    format!("{}{}", DIGITS[da] as char, midpoint(rest_a, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn first_key_and_neighbours() {
        let first = between(None, None).unwrap();
        assert!(is_valid(&first));
        let after = between(Some(&first), None).unwrap();
        let before = between(None, Some(&first)).unwrap();
        assert!(before < first && first < after, "{before} {first} {after}");
    }

    #[test]
    fn adjacent_digits_extend_the_key() {
        let k = between(Some("V"), Some("W")).unwrap();
        assert!("V" < k.as_str() && k.as_str() < "W", "{k}");
        let k = between(Some("z"), None).unwrap();
        assert!("z" < k.as_str(), "{k}");
        let k = between(None, Some("1")).unwrap();
        assert!(k.as_str() < "1" && is_valid(&k), "{k}");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(between(Some("b"), Some("a")).is_err());
        assert!(between(Some("a"), Some("a")).is_err());
        assert!(between(Some("a0"), None).is_err());
        assert!(between(Some(""), None).is_err());
        assert!(between(Some("a-b"), None).is_err());
    }

    #[test]
    fn spread_is_increasing() {
        let keys = spread(200);
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        assert!(keys.iter().all(|k| is_valid(k)));
    }

    #[test]
    fn repeated_inserts_at_one_spot_never_collide() {
        // The float scheme collapsed after ~50 of these.
        let lo = between(None, None).unwrap();
        let mut hi = between(Some(&lo), None).unwrap();
        for _ in 0..500 {
            let mid = between(Some(&lo), Some(&hi)).unwrap();
            assert!(lo < mid && mid < hi);
            hi = mid;
        }
    }

    proptest! {
        /// Any sequence of insertions keeps every key valid, unique and in
        /// the order the insertions asked for.
        #[test]
        fn random_insertions_keep_order(slots in proptest::collection::vec(any::<prop::sample::Index>(), 1..120)) {
            let mut keys: Vec<String> = Vec::new();
            for slot in slots {
                let i = slot.index(keys.len() + 1);
                let before = if i == 0 { None } else { Some(keys[i - 1].as_str()) };
                let after = keys.get(i).map(String::as_str);
                let k = between(before, after).unwrap();
                prop_assert!(is_valid(&k));
                keys.insert(i, k);
            }
            prop_assert!(keys.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
