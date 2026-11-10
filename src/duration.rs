//! [`DurationExt`], which builds a [`Duration`] from an integer, such as `30.seconds()`.

use core::time::Duration;

/// Builds a [`Duration`] from an integer, for example `250.milliseconds()` or `30.seconds()`.
///
/// It is implemented for `u64` only, so that an integer literal without a suffix picks it. Unlike
/// `Duration::from_mins` and `Duration::from_hours`, `minutes` and `hours` do not panic for huge
/// numbers, but saturate at [`Duration::MAX`].
///
/// The trait is sealed, so it can gain methods without breaking code that uses it.
pub trait DurationExt: sealed::Sealed {
    /// Returns a duration of this many nanoseconds.
    #[must_use]
    fn nanoseconds(self) -> Duration;

    /// Returns a duration of this many microseconds.
    #[must_use]
    fn microseconds(self) -> Duration;

    /// Returns a duration of this many milliseconds.
    #[must_use]
    fn milliseconds(self) -> Duration;

    /// Returns a duration of this many seconds.
    #[must_use]
    fn seconds(self) -> Duration;

    /// Returns a duration of this many minutes, at most [`Duration::MAX`].
    #[must_use]
    fn minutes(self) -> Duration;

    /// Returns a duration of this many hours, at most [`Duration::MAX`].
    #[must_use]
    fn hours(self) -> Duration;
}

impl DurationExt for u64 {
    fn nanoseconds(self) -> Duration {
        Duration::from_nanos(self)
    }

    fn microseconds(self) -> Duration {
        Duration::from_micros(self)
    }

    fn milliseconds(self) -> Duration {
        Duration::from_millis(self)
    }

    fn seconds(self) -> Duration {
        Duration::from_secs(self)
    }

    fn minutes(self) -> Duration {
        self.checked_mul(60)
            .map_or(Duration::MAX, Duration::from_secs)
    }

    fn hours(self) -> Duration {
        self.checked_mul(60 * 60)
            .map_or(Duration::MAX, Duration::from_secs)
    }
}

mod sealed {
    pub trait Sealed {}

    impl Sealed for u64 {}
}

#[cfg(test)]
mod tests {
    use super::DurationExt;
    use assertr::prelude::*;
    use std::time::Duration;

    #[test]
    fn builds_durations() {
        assert_that!(5.nanoseconds()).is_equal_to(Duration::from_nanos(5));
        assert_that!(5.microseconds()).is_equal_to(Duration::from_micros(5));
        assert_that!(5.milliseconds()).is_equal_to(Duration::from_millis(5));
        assert_that!(5.seconds()).is_equal_to(Duration::from_secs(5));
        assert_that!(5.minutes()).is_equal_to(Duration::from_secs(5 * 60));
        assert_that!(5.hours()).is_equal_to(Duration::from_secs(5 * 60 * 60));
    }

    #[test]
    fn minutes_and_hours_saturate() {
        assert_that!(u64::MAX.minutes()).is_equal_to(Duration::MAX);
        assert_that!(u64::MAX.hours()).is_equal_to(Duration::MAX);
    }
}
