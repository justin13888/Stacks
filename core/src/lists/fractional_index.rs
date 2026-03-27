/// Returns a fractional index value between `before` and `after`.
///
/// If `before` is `None`, uses 0.0 as the lower bound.
/// If `after` is `None`, uses `before + 1.0` (or 1.0 for the very first item).
/// Otherwise returns the midpoint between `before` and `after`.
pub fn midpoint(before: Option<f64>, after: Option<f64>) -> f64 {
    match (before, after) {
        (None, None) => 1.0,
        (Some(b), None) => b + 1.0,
        (None, Some(a)) => a / 2.0,
        (Some(b), Some(a)) => (b + a) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midpoint_first_item() {
        assert_eq!(midpoint(None, None), 1.0);
    }

    #[test]
    fn test_midpoint_append() {
        assert_eq!(midpoint(Some(2.0), None), 3.0);
    }

    #[test]
    fn test_midpoint_prepend() {
        assert_eq!(midpoint(None, Some(2.0)), 1.0);
    }

    #[test]
    fn test_midpoint_between() {
        assert_eq!(midpoint(Some(1.0), Some(3.0)), 2.0);
    }
}
