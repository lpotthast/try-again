//! [`Fixed`], the strategy that waits the same interval after every failure.

use super::{DelaysExt, Forever, TakeTotal};
use core::iter::{FusedIterator, Take};
use core::time::Duration;

/// A strategy that waits the same interval after every failure.
///
/// Limit the strategy first, and shape its delays after that: `.take(5).jitter(random)`; see
/// [`jitter`](super::DelaysExt::jitter).
///
/// ```
/// # #[cfg(feature = "std")] {
/// use try_again::delay::Fixed;
/// use try_again::{retry, DurationExt};
///
/// let mut calls = 0;
/// let error = retry(|| {
///     calls += 1;
///     Err::<(), _>(calls)
/// })
/// .delays(Fixed::of(1.milliseconds()).take(2))
/// .run()
/// .unwrap_err();
///
/// // The first try plus two retries.
/// assert_eq!(error.tries, 3);
/// assert_eq!(error.failure, 3);
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fixed {
    interval: Duration,
}

impl Fixed {
    /// Creates a strategy that waits `interval` after every failure.
    #[must_use]
    pub const fn of(interval: Duration) -> Self {
        Self { interval }
    }

    /// Returns the delay after every failure.
    #[must_use]
    pub const fn interval(&self) -> Duration {
        self.interval
    }

    /// Returns the first `retries` delays: a retry with them tries again at most `retries` times.
    pub fn take(self, retries: usize) -> Take<FixedDelays> {
        self.delays().take(retries)
    }

    /// Returns the delays while their sum stays within `budget`; see [`DelaysExt::take_total`].
    ///
    /// # Panics
    ///
    /// Panics if the interval is zero: zero delays never use up a budget, so the retry would never
    /// end. Limit immediate retries with [`take`](Fixed::take) instead.
    pub fn take_total(self, budget: Duration) -> TakeTotal<FixedDelays> {
        assert!(
            !self.interval.is_zero(),
            "zero delays never use up a budget; limit them with `take(n)` instead"
        );
        self.delays().take_total(budget)
    }

    /// Returns all delays, explicitly without a limit; see [`ExplicitLimit`](super::ExplicitLimit).
    pub fn forever(self) -> Forever<FixedDelays> {
        self.delays().forever()
    }

    /// The infinite sequence of delays.
    const fn delays(self) -> FixedDelays {
        FixedDelays {
            interval: self.interval,
        }
    }
}

/// The infinite iterator of a [`Fixed`] strategy's delays, which its [`take`](Fixed::take),
/// [`take_total`](Fixed::take_total) and [`forever`](Fixed::forever) wrap.
#[derive(Debug, Clone)]
#[must_use = "delays do nothing unless a retry waits for them"]
pub struct FixedDelays {
    interval: Duration,
}

impl Iterator for FixedDelays {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        Some(self.interval)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }
}

impl FusedIterator for FixedDelays {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use assertr::prelude::*;

    #[test]
    fn keeps_its_interval() {
        assert_that!(Fixed::of(500.milliseconds()).interval()).is_equal_to(500.milliseconds());
    }

    #[test]
    fn take_limits_the_number_of_delays() {
        let delays: Vec<Duration> = Fixed::of(1.seconds()).take(3).collect();

        assert_that!(delays).is_equal_to(vec![1.seconds(); 3]);
    }

    #[test]
    fn take_total_limits_the_sum_of_the_delays() {
        let delays: Vec<Duration> = Fixed::of(3.seconds()).take_total(10.seconds()).collect();

        assert_that!(delays).is_equal_to(vec![3.seconds(); 3]);
    }

    #[test]
    #[should_panic(expected = "zero delays never use up a budget")]
    fn take_total_rejects_a_zero_interval() {
        let _ = Fixed::of(Duration::ZERO).take_total(1.seconds());
    }

    #[test]
    fn forever_repeats_the_interval() {
        let delays: Vec<Duration> = Fixed::of(1.seconds()).forever().take(2).collect();

        assert_that!(delays).is_equal_to(vec![1.seconds(); 2]);
    }
}
