//! [`Exponential`], the strategy whose delays grow by a factor after each failure.

use super::{DelaysExt, Forever, TakeTotal};
use core::iter::{FusedIterator, Take};
use core::time::Duration;

/// A strategy whose delay grows by a factor after each consecutive failure, optionally up to a
/// cap.
///
/// The factor is 2 unless [`with_factor`](Exponential::with_factor) changes it. Limit the strategy
/// first, and shape its delays after that: `.take(5).jitter(random)`; see
/// [`jitter`](super::DelaysExt::jitter).
///
/// ```
/// use try_again::DurationExt;
/// use try_again::delay::Exponential;
///
/// let strategy = Exponential::of(1.seconds()).capped_at(30.seconds());
/// let delays: Vec<_> = strategy.take(4).collect();
///
/// assert_eq!(delays, [1.seconds(), 2.seconds(), 4.seconds(), 8.seconds()]);
///
/// assert_eq!(strategy.delay_after(1), 1.seconds());
/// assert_eq!(strategy.delay_after(3), 4.seconds());
/// assert_eq!(strategy.delay_after(10), 30.seconds());
///
/// let gentle = Exponential::of(1.seconds()).with_factor(1.5);
///
/// assert_eq!(gentle.delay_after(3), 2250.milliseconds());
/// ```
///
/// Uncapped delays grow fast: starting at one second and doubling, the 18th delay is longer than a
/// day, and delays eventually saturate at [`Duration::MAX`]. Cap a strategy that may see many
/// failures, especially with `forever()`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exponential {
    first: Duration,
    factor: f64,
    cap: Duration,
}

impl Exponential {
    /// Creates a strategy that waits `first` after the first failure, and doubles the delay after
    /// each further one, without a cap.
    ///
    /// # Panics
    ///
    /// Panics if `first` is zero, as all delays would then be zero. Use
    /// [`Fixed`](super::Fixed) for immediate retries.
    #[must_use]
    pub const fn of(first: Duration) -> Self {
        assert!(
            !first.is_zero(),
            "the first delay of an exponential backoff must not be zero"
        );
        Self {
            first,
            factor: 2.0,
            cap: Duration::MAX,
        }
    }

    /// Returns the same strategy, growing by `factor` instead of doubling.
    ///
    /// # Panics
    ///
    /// Panics if `factor` is less than 1 or not finite.
    #[must_use]
    pub const fn with_factor(self, factor: f64) -> Self {
        assert!(
            factor.is_finite() && factor >= 1.0,
            "the factor of an exponential backoff must be finite and at least 1"
        );
        Self {
            first: self.first,
            factor,
            cap: self.cap,
        }
    }

    /// Returns the same strategy with every delay capped at `cap`, including the first one.
    ///
    /// # Panics
    ///
    /// Panics if `cap` is zero, as all delays would then be zero. Use [`Fixed`](super::Fixed) for
    /// immediate retries.
    #[must_use]
    pub const fn capped_at(self, cap: Duration) -> Self {
        assert!(
            !cap.is_zero(),
            "the cap of an exponential backoff must not be zero"
        );
        Self {
            first: self.first,
            factor: self.factor,
            cap,
        }
    }

    /// Returns the delay after the first failure, before the cap applies.
    #[must_use]
    pub const fn first(&self) -> Duration {
        self.first
    }

    /// Returns the factor by which the delay grows after each failure.
    #[must_use]
    pub const fn factor(&self) -> f64 {
        self.factor
    }

    /// Returns the longest delay, [`Duration::MAX`] for an uncapped strategy.
    #[must_use]
    pub const fn cap(&self) -> Duration {
        self.cap
    }

    /// Returns the delay after `failures` consecutive failures: `first · factor^(failures − 1)`,
    /// at most the cap. Zero failures count as one.
    ///
    /// A loop of your own can count its failures, persist the count if it must survive a restart,
    /// and ask for the delay directly; see [the module](super#delays-in-a-loop-of-your-own).
    #[must_use]
    pub fn delay_after(&self, failures: u32) -> Duration {
        // Exact, even for a first delay that `f64` cannot hold in nanoseconds. The factor is at
        // least 1, so a factor of at most 1 is 1, and keeps the first delay.
        if failures <= 1 || self.factor <= 1.0 {
            return self.first.min(self.cap);
        }
        // Exact for a power-of-two factor while the delay stays below about 52 days. A product
        // that is infinite, or too large for a `Duration`, saturates.
        let grown = self.first.as_secs_f64() * power(self.factor, failures - 1);
        let delay = Duration::try_from_secs_f64(grown).unwrap_or(Duration::MAX);
        delay.min(self.cap)
    }

    /// Returns the first `retries` delays: a retry with them tries again at most `retries` times.
    pub fn take(self, retries: usize) -> Take<ExponentialDelays> {
        self.delays().take(retries)
    }

    /// Returns the delays while their sum stays within `budget`; see [`DelaysExt::take_total`].
    pub fn take_total(self, budget: Duration) -> TakeTotal<ExponentialDelays> {
        self.delays().take_total(budget)
    }

    /// Returns all delays, explicitly without a limit; see [`ExplicitLimit`](super::ExplicitLimit).
    pub fn forever(self) -> Forever<ExponentialDelays> {
        self.delays().forever()
    }

    /// The infinite sequence of delays.
    const fn delays(self) -> ExponentialDelays {
        ExponentialDelays {
            strategy: self,
            failures: 0,
        }
    }
}

/// `base` to the power of `exponent`, by squaring, because `powi` needs `std`. Like `powi`, it is
/// exact for a base that is a power of two, until the result overflows to infinity.
fn power(base: f64, mut exponent: u32) -> f64 {
    let mut result = 1.0;
    let mut square = base;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result *= square;
        }
        exponent >>= 1;
        if exponent > 0 {
            square *= square;
        }
    }
    result
}

/// The infinite iterator of an [`Exponential`] strategy's delays, which its
/// [`take`](Exponential::take), [`take_total`](Exponential::take_total) and
/// [`forever`](Exponential::forever) wrap.
#[derive(Debug, Clone)]
#[must_use = "delays do nothing unless a retry waits for them"]
pub struct ExponentialDelays {
    strategy: Exponential,
    failures: u32,
}

impl Iterator for ExponentialDelays {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        self.failures = self.failures.saturating_add(1);
        Some(self.strategy.delay_after(self.failures))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }
}

impl FusedIterator for ExponentialDelays {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use assertr::prelude::*;

    #[test]
    fn power_is_exact_for_powers_of_two() {
        assert_that!(power(2.0, 0)).is_equal_to(1.0);
        assert_that!(power(2.0, 10)).is_equal_to(1024.0);
        assert_that!(power(2.0, 62)).is_equal_to(4_611_686_018_427_387_904.0);
        assert_that!(power(2.0, 2000)).is_equal_to(f64::INFINITY);
    }

    #[test]
    fn power_matches_powi() {
        for factor in [1.0_f64, 1.1, 1.5, 3.0, 10.0] {
            for exponent in 0..64 {
                let expected = factor.powi(i32::try_from(exponent).unwrap_or(i32::MAX));
                let relative_error = (power(factor, exponent) - expected).abs() / expected;

                assert_that!(relative_error < 1e-12).is_true();
            }
        }
    }

    #[test]
    fn doubles_from_the_first_delay_up_to_the_cap() {
        // Arrange
        let strategy = Exponential::of(1.seconds()).capped_at(30.seconds());

        // Act
        let delays = (1..=7).map(|failures| strategy.delay_after(failures));

        // Assert
        assert_that_owned!(delays).contains_exactly([
            1.seconds(),
            2.seconds(),
            4.seconds(),
            8.seconds(),
            16.seconds(),
            30.seconds(),
            30.seconds(),
        ]);
    }

    #[test]
    fn no_failure_yet_gives_the_first_delay() {
        let strategy = Exponential::of(250.milliseconds()).capped_at(1.seconds());

        assert_that!(strategy.delay_after(0)).is_equal_to(250.milliseconds());
    }

    #[test]
    fn the_cap_applies_to_the_first_delay_too() {
        let strategy = Exponential::of(5.seconds()).capped_at(1.seconds());

        let delays = [0, 1, 2].map(|failures| strategy.delay_after(failures));

        assert_that!(delays).is_equal_to([1.seconds(); 3]);
    }

    #[test]
    fn without_a_cap_saturates_at_the_maximum_duration() {
        // Arrange
        let strategy = Exponential::of(1.seconds());

        // Act
        let delays = [31, 33, 64, 65, u32::MAX].map(|failures| strategy.delay_after(failures));

        // Assert
        assert_that!(strategy.cap()).is_equal_to(Duration::MAX);
        assert_that!(delays).is_equal_to([
            Duration::from_secs(1 << 30),
            Duration::from_secs(1 << 32),
            Duration::from_secs(1 << 63),
            Duration::MAX,
            Duration::MAX,
        ]);
    }

    #[test]
    fn the_first_delay_is_exact() {
        let first = Duration::from_nanos(u64::MAX);

        assert_that!(Exponential::of(first).delay_after(1)).is_equal_to(first);
    }

    #[test]
    #[should_panic(expected = "the first delay of an exponential backoff must not be zero")]
    fn rejects_a_zero_first_delay() {
        let _ = Exponential::of(Duration::ZERO);
    }

    #[test]
    #[should_panic(expected = "the cap of an exponential backoff must not be zero")]
    fn rejects_a_zero_cap() {
        let _ = Exponential::of(1.seconds()).capped_at(Duration::ZERO);
    }

    #[test]
    fn keeps_its_first_delay_factor_and_cap() {
        let strategy = Exponential::of(1.seconds())
            .with_factor(3.0)
            .capped_at(9.seconds());

        assert_that!(strategy.first()).is_equal_to(1.seconds());
        assert_that!(strategy.factor()).is_equal_to(3.0);
        assert_that!(strategy.cap()).is_equal_to(9.seconds());
    }

    #[test]
    fn grows_by_its_factor() {
        // Arrange
        let strategy = Exponential::of(100.milliseconds()).with_factor(1.5);

        // Act
        let delays = (1..=4).map(|failures| strategy.delay_after(failures));

        // Assert
        assert_that_owned!(delays).contains_exactly([
            100.milliseconds(),
            150.milliseconds(),
            225.milliseconds(),
            Duration::from_micros(337_500),
        ]);
    }

    #[test]
    fn a_factor_of_one_keeps_the_first_delay() {
        // 200 days and a nanosecond, more than an `f64` holds in nanoseconds without rounding.
        let first = Duration::new(200 * 24 * 60 * 60, 1);
        let strategy = Exponential::of(first).with_factor(1.0);

        let delays = [1, 2, u32::MAX].map(|failures| strategy.delay_after(failures));

        assert_that!(delays).is_equal_to([first; 3]);
    }

    #[test]
    #[should_panic(expected = "must be finite and at least 1")]
    fn rejects_a_factor_below_one() {
        let _ = Exponential::of(1.seconds()).with_factor(0.5);
    }

    #[test]
    #[should_panic(expected = "must be finite and at least 1")]
    fn rejects_a_nan_factor() {
        let _ = Exponential::of(1.seconds()).with_factor(f64::NAN);
    }

    mod delays {
        use super::*;

        #[test]
        fn follow_delay_after() {
            let strategy = Exponential::of(50.milliseconds()).capped_at(150.milliseconds());

            let delays: Vec<Duration> = strategy.take(4).collect();

            assert_that!(delays).contains_exactly([
                50.milliseconds(),
                100.milliseconds(),
                150.milliseconds(),
                150.milliseconds(),
            ]);
        }

        #[test]
        fn continue_at_the_cap_without_an_end() {
            // Arrange
            let mut delays = Exponential::of(1.seconds())
                .capped_at(2.seconds())
                .forever();

            // Act
            let late_delay = delays.nth(1_000);

            // Assert
            assert_that!(late_delay).is_equal_to(Some(2.seconds()));
        }

        #[test]
        fn of_a_clone_continue_independently() {
            // Arrange
            let mut delays = Exponential::of(1.seconds()).forever();
            delays.next();

            // Act
            let mut copy = delays.clone();
            let original_next = delays.next();
            let copy_next = copy.next();

            // Assert
            assert_that!(original_next).is_equal_to(Some(2.seconds()));
            assert_that!(copy_next).is_equal_to(Some(2.seconds()));
        }
    }
}
