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
    Ok(match (before, after) {
        (Some(a), None) => successor(a.as_bytes()),
        (None, Some(b)) => predecessor(b.as_bytes()),
        _ => midpoint(before.unwrap_or("").as_bytes(), after.map(str::as_bytes)),
    })
}

/// `n` evenly spread, increasing keys — used to lay out a whole list afresh.
///
/// Key `i` (from 1) is the fraction `i / (n + 1)` truncated to the fewest
/// digits that keep every key distinct, so no key is longer than
/// `ceil(log62(n + 1))` digits.
pub fn spread(n: usize) -> Vec<String> {
    let slots = n as u128 + 1;
    let mut width = 1;
    let mut scale = BASE as u128;
    while scale < slots {
        width += 1;
        scale *= BASE as u128;
    }
    (1..slots)
        .map(|i| {
            let mut value = i * scale / slots;
            let mut digits = vec![0; width];
            for d in digits.iter_mut().rev() {
                *d = (value % BASE as u128) as usize;
                value /= BASE as u128;
            }
            encode(&[], &digits)
        })
        .collect()
}

/// The widest head [`successor`] and [`predecessor`] step: a block of keys
/// sharing one run of leading `z`s (or `0`s) then holds up to 62^8 keys,
/// far more than any list, so a key never grows past its run plus this.
const MAX_HEAD: usize = 8;

/// How many digits after a run of `run` leading `z`s (or `0`s) a step
/// counts in. Widening by one digit per extra run digit is what makes key
/// length grow with the logarithm of the number of appends (or prepends)
/// rather than linearly: each block holds 62 times the keys of the last.
fn head_width(run: usize) -> usize {
    (run + 1).min(MAX_HEAD)
}

/// The leading run of `pad` in `key`, and the next `width` digits after it
/// (zero-padded), as digit values.
fn split_head(key: &[u8], pad: u8) -> (usize, Vec<usize>) {
    let run = key.iter().take_while(|&&d| d == pad).count();
    let width = head_width(run);
    let head = key[run..]
        .iter()
        .map(|&d| digit_value(d).unwrap_or(0))
        .chain(std::iter::repeat(0))
        .take(width)
        .collect();
    (run, head)
}

/// `prefix` followed by `digits`, without trailing `0`s (which leave the
/// fraction unchanged).
fn encode(prefix: &[u8], digits: &[usize]) -> String {
    let end = digits.iter().rposition(|&d| d != 0).map_or(0, |i| i + 1);
    let mut out = String::from_utf8_lossy(prefix).into_owned();
    out.extend(digits[..end].iter().map(|&d| DIGITS[d] as char));
    out
}

/// A key just after `a`: its head (the digits after any leading `z` run)
/// plus one unit in the head's last digit.
fn successor(a: &[u8]) -> String {
    let (run, mut head) = split_head(a, DIGITS[BASE - 1]);
    // The head's first digit is not `z`, so this carry stops inside it.
    for d in head.iter_mut().rev() {
        *d += 1;
        if *d < BASE {
            break;
        }
        *d = 0;
    }
    encode(&a[..run], &head)
}

/// A key just before `b`: its head (the digits after any leading `0` run),
/// cut short where `b` goes on past it, else minus one unit in its last
/// digit.
fn predecessor(b: &[u8]) -> String {
    let (run, mut head) = split_head(b, DIGITS[0]);
    if b.len() - run <= head.len() {
        // The head's first digit is not `0`, so this borrow stops inside it.
        for d in head.iter_mut().rev() {
            if *d > 0 {
                *d -= 1;
                break;
            }
            *d = BASE - 1;
        }
    }
    let key = encode(&b[..run], &head);
    if is_valid(&key) {
        key
    } else {
        // `b` was `1` after its zeros: open the next block at its top.
        let mut prefix = b[..=run].to_vec();
        prefix[run] = DIGITS[0];
        encode(&prefix, &vec![BASE - 1; head_width(run + 1)])
    }
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
    fn spread_keys_stay_short() {
        for n in [0, 1, 61, 62, 3843, 3844, 10_000] {
            let keys = spread(n);
            assert_eq!(keys.len(), n);
            assert!(keys.windows(2).all(|w| w[0] < w[1]), "n = {n}");
            assert!(keys.iter().all(|k| is_valid(k)), "n = {n}");
            // ceil(log62(n + 1)) digits.
            let width = if n < 62 {
                1
            } else if n < 3844 {
                2
            } else {
                3
            };
            assert!(keys.iter().all(|k| k.len() <= width), "n = {n}");
        }
    }

    /// Key lengths grow with the logarithm of the list, not linearly: at
    /// ~1 char per 5 items the 10,000th append was ~2,000 chars long.
    #[test]
    fn ten_thousand_appends_stay_short() {
        let mut last = between(None, None).unwrap();
        for _ in 0..10_000 {
            let next = between(Some(&last), None).unwrap();
            assert!(last < next && is_valid(&next), "{last} {next}");
            last = next;
        }
        assert!(last.len() <= 5, "{last}");
    }

    #[test]
    fn ten_thousand_prepends_stay_short() {
        let mut first = between(None, None).unwrap();
        for _ in 0..10_000 {
            let next = between(None, Some(&first)).unwrap();
            assert!(next < first && is_valid(&next), "{next} {first}");
            first = next;
        }
        assert!(first.len() <= 5, "{first}");
    }

    #[test]
    fn steps_past_long_existing_keys() {
        // Keys the old generator wrote keep sorting, and the next key is at
        // most the run of `z`s (or `0`s) plus a bounded head.
        let long = format!("{}V", "z".repeat(200));
        let next = between(Some(&long), None).unwrap();
        assert!(long < next && is_valid(&next));
        assert!(next.len() <= 200 + MAX_HEAD, "{}", next.len());
        let long = format!("{}1", "0".repeat(200));
        let prev = between(None, Some(&long)).unwrap();
        assert!(prev < long && is_valid(&prev));
        assert!(prev.len() <= 201 + MAX_HEAD, "{}", prev.len());
        assert_eq!(between(None, Some("1")).unwrap(), "0zz");
        assert_eq!(between(Some("z"), None).unwrap(), "z01");
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
