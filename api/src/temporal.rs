//! Instants and dates at the API edge.
//!
//! core and the database keep chrono; the API carries jiff, which kynos
//! describes as the same `date-time` and `date` schemas (its chrono mapping
//! is deprecated, chrono being unmaintained). These convert at the boundary,
//! in both directions.
//!
//! Every jiff value has a chrono equivalent. chrono's range is wider than
//! jiff's (years -9999 to 9999), so a chrono value outside it, which nothing
//! Stacks stores reaches, saturates at jiff's bound rather than failing.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use jiff::Timestamp;
use jiff::civil::Date;

const NANOS_PER_SECOND: i32 = 1_000_000_000;

/// An instant as the API carries it.
pub fn timestamp<Tz: TimeZone>(t: DateTime<Tz>) -> Timestamp {
    let seconds = t.timestamp();
    // chrono spells a leap second as a nanosecond count past one second;
    // jiff has no leap seconds, so it is the last instant of that second.
    let nanos = i32::try_from(t.timestamp_subsec_nanos())
        .unwrap_or(NANOS_PER_SECOND - 1)
        .min(NANOS_PER_SECOND - 1);
    Timestamp::new(seconds, nanos).unwrap_or(if seconds < 0 {
        Timestamp::MIN
    } else {
        Timestamp::MAX
    })
}

/// An instant the API was given, as core takes it.
pub fn instant(t: Timestamp) -> DateTime<Utc> {
    // jiff truncates toward zero, so an instant before 1970 has a negative
    // fraction; chrono wants the floor second and a positive fraction.
    let (seconds, nanos) = match t.subsec_nanosecond() {
        n if n < 0 => (t.as_second() - 1, n + NANOS_PER_SECOND),
        n => (t.as_second(), n),
    };
    let nanos = u32::try_from(nanos).unwrap_or_default();
    DateTime::from_timestamp(seconds, nanos).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

/// A calendar date as the API carries it.
pub fn date(d: NaiveDate) -> Date {
    let (Ok(year), Ok(month), Ok(day)) = (
        i16::try_from(d.year()),
        i8::try_from(d.month()),
        i8::try_from(d.day()),
    ) else {
        return if d.year() < 0 { Date::MIN } else { Date::MAX };
    };
    Date::new(year, month, day).unwrap_or(if year < 0 { Date::MIN } else { Date::MAX })
}

/// A calendar date the API was given, as core takes it.
pub fn naive_date(d: Date) -> NaiveDate {
    let month = u32::try_from(d.month()).unwrap_or(1);
    let day = u32::try_from(d.day()).unwrap_or(1);
    NaiveDate::from_ymd_opt(i32::from(d.year()), month, day).unwrap_or(NaiveDate::MIN)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn utc(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339).unwrap().to_utc()
    }

    #[test]
    fn an_instant_round_trips_to_the_nanosecond() {
        for raw in [
            "2024-03-01T12:34:56.123456789Z",
            "1970-01-01T00:00:00Z",
            "1969-12-31T23:59:59.5Z",
            "1900-06-15T08:00:00.000001Z",
        ] {
            let t = utc(raw);
            assert_eq!(instant(timestamp(t)), t, "{raw}");
            assert_eq!(
                timestamp(t).to_string().parse::<Timestamp>().unwrap(),
                timestamp(t)
            );
        }
    }

    #[test]
    fn an_offset_instant_is_the_same_instant() {
        let offset =
            DateTime::<FixedOffset>::parse_from_rfc3339("2024-03-01T14:00:00+02:00").unwrap();
        assert_eq!(
            timestamp(offset),
            "2024-03-01T12:00:00Z".parse::<Timestamp>().unwrap()
        );
    }

    #[test]
    fn a_leap_second_is_the_last_instant_of_its_second() {
        let leap = NaiveDate::from_ymd_opt(2016, 12, 31)
            .unwrap()
            .and_hms_nano_opt(23, 59, 59, 1_500_000_000)
            .unwrap()
            .and_utc();
        assert_eq!(
            timestamp(leap),
            "2016-12-31T23:59:59.999999999Z"
                .parse::<Timestamp>()
                .unwrap()
        );
    }

    #[test]
    fn an_instant_beyond_jiffs_range_saturates() {
        let far = NaiveDate::from_ymd_opt(20_000, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let early = NaiveDate::from_ymd_opt(-20_000, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert_eq!(timestamp(far), Timestamp::MAX);
        assert_eq!(timestamp(early), Timestamp::MIN);
    }

    #[test]
    fn a_date_round_trips() {
        for (y, m, d) in [(2024, 2, 29), (1, 1, 1), (-44, 3, 15), (9999, 12, 31)] {
            let naive = NaiveDate::from_ymd_opt(y, m, d).unwrap();
            assert_eq!(naive_date(date(naive)), naive, "{naive}");
        }
        assert_eq!(
            date(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()).to_string(),
            "2024-02-29"
        );
    }

    #[test]
    fn a_date_beyond_jiffs_range_saturates() {
        assert_eq!(
            date(NaiveDate::from_ymd_opt(20_000, 1, 1).unwrap()),
            Date::MAX
        );
        assert_eq!(
            date(NaiveDate::from_ymd_opt(-20_000, 1, 1).unwrap()),
            Date::MIN
        );
    }
}
