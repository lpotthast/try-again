//! [`retry_after`], which reads the `Retry-After` header of an HTTP response.

use std::time::{Duration, SystemTime};

/// Reads the value of an HTTP `Retry-After` header: the delay that the server asks for, either in
/// seconds or until an HTTP date.
///
/// Use it in [`override_next_delay`](crate::Retry::override_next_delay); see the recipe there.
///
/// A date is measured from `now`, usually `SystemTime::now()`, and a date that already passed asks
/// for no delay. Taking `now` as a parameter keeps the retry free of clock reads, and makes tests
/// reproducible. In the browser (`wasm32-unknown-unknown`), where `SystemTime::now()` panics, take
/// it from JavaScript's clock: `UNIX_EPOCH + Duration::from_millis(js_sys::Date::now() as u64)`.
///
/// Returns `None` if the value is neither a number of seconds nor an HTTP date, in any of the
/// three formats that HTTP allows.
///
/// ```
/// use std::time::{Duration, SystemTime};
/// use try_again::delay::retry_after;
/// use try_again::DurationExt;
///
/// let now = SystemTime::UNIX_EPOCH + 784_111_717.seconds(); // Sun, 06 Nov 1994 08:48:37 GMT
///
/// assert_eq!(retry_after("120", now), Some(2.minutes()));
/// assert_eq!(retry_after("Sun, 06 Nov 1994 08:49:37 GMT", now), Some(1.minutes()));
/// assert_eq!(retry_after("Sun, 06 Nov 1994 08:47:37 GMT", now), Some(Duration::ZERO));
/// assert_eq!(retry_after("soon", now), None);
/// ```
#[must_use]
pub fn retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        // A number of seconds too large for a `u64` asks for more than any delay.
        return Some(value.parse().map_or(Duration::MAX, Duration::from_secs));
    }
    let date = httpdate::parse_http_date(value).ok()?;
    Some(date.duration_since(now).unwrap_or(Duration::ZERO))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use assertr::prelude::*;

    /// Sun, 06 Nov 1994 08:48:37 GMT, a minute before the dates of RFC 9110's examples.
    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + 784_111_717.seconds()
    }

    #[test]
    fn reads_seconds() {
        assert_that!(retry_after("0", now())).is_equal_to(Some(Duration::ZERO));
        assert_that!(retry_after(" 120 ", now())).is_equal_to(Some(2.minutes()));
    }

    #[test]
    fn saturates_seconds_beyond_the_longest_duration() {
        let value = "99999999999999999999999";

        assert_that!(retry_after(value, now())).is_equal_to(Some(Duration::MAX));
    }

    #[test]
    fn reads_a_date_in_each_format_of_http() {
        let dates = [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
        ];

        let delays = dates.map(|date| retry_after(date, now()));

        assert_that!(delays).is_equal_to([Some(1.minutes()); 3]);
    }

    #[test]
    fn a_date_that_passed_asks_for_no_delay() {
        let delay = retry_after("Sun, 06 Nov 1994 08:47:37 GMT", now());

        assert_that!(delay).is_equal_to(Some(Duration::ZERO));
    }

    #[test]
    fn rejects_other_values() {
        for value in ["", "soon", "-5", "1.5", "Sun, 06 Nov 1994"] {
            assert_that!(retry_after(value, now())).is_none();
        }
    }
}
