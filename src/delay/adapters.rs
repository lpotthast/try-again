//! The adapters of delays: [`DelaysExt::forever`], [`DelaysExt::take_total`] and
//! [`DelaysExt::jitter`].

use super::ExplicitLimit;
use core::fmt::{self, Debug, Formatter};
use core::iter::FusedIterator;
use core::time::Duration;

/// Adds [`forever`](DelaysExt::forever), [`take_total`](DelaysExt::take_total) and
/// [`jitter`](DelaysExt::jitter) to every `IntoIterator<Item = Duration>`.
///
/// [`Fixed`](super::Fixed) and [`Exponential`](super::Exponential) are not iterators: their own
/// `take`, `take_total` and `forever` return their delays, which this trait then adapts.
pub trait DelaysExt: IntoIterator<Item = Duration> + Sized {
    /// Marks the delays as deliberately unlimited, so a retry accepts them.
    ///
    /// The retry tries again for as long as the delays last, and ends when finite delays end. See
    /// [`ExplicitLimit`] for when a retry with infinite delays ends.
    ///
    /// ```
    /// use std::iter;
    /// use try_again::delay::DelaysExt;
    /// use try_again::{retry, DurationExt};
    ///
    /// // `repeat` never ends, so the retry gives up only when `when` rejects a failure.
    /// let mut patience = 3;
    /// let out = retry(|| Err::<(), ()>(()))
    ///     .when(|()| {
    ///         patience -= 1;
    ///         patience > 0
    ///     })
    ///     .sleep_with(|_delay| {})
    ///     .delays(iter::repeat(1.seconds()).forever())
    ///     .run();
    ///
    /// assert_eq!(out.unwrap_err().tries, 3);
    /// ```
    fn forever(self) -> Forever<Self::IntoIter> {
        Forever {
            delays: self.into_iter(),
        }
    }

    /// Takes delays while their sum stays within `budget`: a retry with them waits at most
    /// `budget` in total, unless `override_next_delay` lengthens its delays.
    ///
    /// The delays end before the first delay that would take their sum beyond `budget`, even when
    /// later delays would be shorter. This limits only the waiting, not the time that the tries
    /// take. To limit an async retry by time, tries included, give it a deadline with
    /// [`timeout_total`](crate::Retry::timeout_total).
    ///
    /// The budget counts the delays as they leave this iterator. A delay that
    /// [`override_next_delay`](crate::Retry::override_next_delay) replaces counts as scheduled,
    /// not as overridden, so cap an overridden delay in its closure.
    ///
    /// It states a limit like `take(n)`, and combines with it: `.take(10).take_total(budget)`
    /// stops at whichever limit comes first. A zero delay uses up none of the budget, so delays
    /// that are all zero never reach it; limit those with `take(n)`.
    ///
    /// ```
    /// # #[cfg(feature = "std")] {
    /// use try_again::delay::{DelaysExt, Exponential};
    /// use try_again::{retry, DurationExt};
    ///
    /// // 1 ms, 2 ms and 4 ms fit into 10 ms. The next delay, 8 ms, would not.
    /// let error = retry(|| Err::<(), ()>(()))
    ///     .delays(Exponential::of(1.milliseconds()).take_total(10.milliseconds()))
    ///     .run()
    ///     .unwrap_err();
    ///
    /// assert_eq!(error.tries, 4);
    /// assert_eq!(error.total_delay, 7.milliseconds());
    ///
    /// // Any other delays, for example an array, take a budget through this trait.
    /// let listed: Vec<_> = [3.milliseconds(); 3].take_total(7.milliseconds()).collect();
    /// assert_eq!(listed, [3.milliseconds(); 2]);
    /// # }
    /// ```
    fn take_total(self, budget: Duration) -> TakeTotal<Self::IntoIter> {
        TakeTotal {
            delays: self.into_iter(),
            remaining: Some(budget),
        }
    }

    /// Multiplies each delay by a random factor from `random`, so that clients that failed together
    /// do not retry together.
    ///
    /// `random` returns the factor. For "full jitter", which [AWS recommends][aws], the factor is
    /// in `0.0..1.0`. For "equal jitter", it is in `0.5..1.0`, which keeps at least half of each
    /// delay. With the feature `fastrand`, pass [`delay::random_factor`](super::random_factor) for
    /// full jitter, or `|| 0.5 + 0.5 * delay::random_factor()` for equal jitter. Any other random
    /// number generator works too, for example `rand`, and a deterministic function makes the
    /// delays reproducible in tests.
    ///
    /// [aws]: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
    ///
    /// Keep every factor at most 1, so that jitter only shortens delays: the delays then stay
    /// within the cap of an [`Exponential`](super::Exponential) strategy and the budget of
    /// [`take_total`](DelaysExt::take_total). A larger factor lengthens its delay beyond both, and
    /// a negative or NaN factor gives a zero delay.
    ///
    /// The generator need not be cryptographically secure, but it must be seeded differently for
    /// each client, or clients that failed together get the same factors and still retry together.
    /// In the browser, seed it from the browser's `crypto.getRandomValues`, for example through
    /// `getrandom`'s support for JavaScript: Rust's standard library has no source of randomness
    /// there. `delay::random_factor` does this for you.
    ///
    /// A [`RetryConfig`](crate::RetryConfig) clones its delays for each retry, and with them the
    /// state of `random`. A closure that owns its generator, such as `move || rng.f64()`, would
    /// then draw the same factors in every retry. In a shared config, use a generator that shares
    /// its state, as `delay::random_factor` and `rand::rng()` do.
    ///
    /// Limit the delays first, and add jitter after that. Jittered delays keep the limit of the
    /// delays they adapt:
    ///
    /// ```
    /// use try_again::delay::{DelaysExt, Exponential};
    /// use try_again::DurationExt;
    ///
    /// // A deterministic stand-in for a random number generator.
    /// let mut factors = [0.5, 0.25, 1.0].into_iter().cycle();
    /// let delays: Vec<_> = Exponential::of(100.milliseconds())
    ///     .take(3)
    ///     .jitter(move || factors.next().unwrap_or(1.0))
    ///     .collect();
    ///
    /// assert_eq!(delays, [50.milliseconds(), 50.milliseconds(), 400.milliseconds()]);
    /// ```
    ///
    /// ```compile_fail,E0599
    /// use try_again::delay::{DelaysExt, Exponential};
    /// use try_again::DurationExt;
    ///
    /// // Error: a strategy is no iterator until `take`, `take_total` or `forever` limits it.
    /// let _ = Exponential::of(100.milliseconds()).jitter(|| 0.5).take(3);
    /// ```
    fn jitter<Random>(self, random: Random) -> Jitter<Self::IntoIter, Random>
    where
        Random: FnMut() -> f64,
    {
        Jitter {
            delays: self.into_iter(),
            random,
        }
    }
}

impl<Delays: IntoIterator<Item = Duration>> DelaysExt for Delays {}

/// Delays accepted without a limit, from [`DelaysExt::forever`].
#[derive(Debug, Clone)]
#[must_use = "delays do nothing unless a retry waits for them"]
pub struct Forever<Delays> {
    delays: Delays,
}

impl<Delays: Iterator<Item = Duration>> Iterator for Forever<Delays> {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        self.delays.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.delays.size_hint()
    }
}

impl<Delays: FusedIterator<Item = Duration>> FusedIterator for Forever<Delays> {}

impl<Delays: Iterator<Item = Duration>> ExplicitLimit for Forever<Delays> {}

/// Delays limited by their sum, from [`DelaysExt::take_total`].
#[derive(Debug, Clone)]
#[must_use = "delays do nothing unless a retry waits for them"]
pub struct TakeTotal<Delays> {
    delays: Delays,
    /// The budget left, or `None` once a delay did not fit into it.
    remaining: Option<Duration>,
}

impl<Delays: Iterator<Item = Duration>> Iterator for TakeTotal<Delays> {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        let remaining = self.remaining?;
        // Ending for good once the delays end keeps the adapter fused, whatever the delays.
        let Some(delay) = self.delays.next() else {
            self.remaining = None;
            return None;
        };
        self.remaining = remaining.checked_sub(delay);
        self.remaining.map(|_| delay)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.remaining {
            Some(_) => (0, self.delays.size_hint().1),
            None => (0, Some(0)),
        }
    }
}

impl<Delays: Iterator<Item = Duration>> FusedIterator for TakeTotal<Delays> {}

// A budget states a limit of its own, whatever the delays it adapts.
impl<Delays: Iterator<Item = Duration>> ExplicitLimit for TakeTotal<Delays> {}

/// Delays multiplied by random factors, from [`DelaysExt::jitter`].
#[derive(Clone)]
#[must_use = "delays do nothing unless a retry waits for them"]
pub struct Jitter<Delays, Random> {
    delays: Delays,
    random: Random,
}

impl<Delays: Debug, Random> Debug for Jitter<Delays, Random> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Jitter")
            .field("delays", &self.delays)
            .finish_non_exhaustive()
    }
}

impl<Delays, Random> Iterator for Jitter<Delays, Random>
where
    Delays: Iterator<Item = Duration>,
    Random: FnMut() -> f64,
{
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        let delay = self.delays.next()?;
        Some(scale(delay, (self.random)()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.delays.size_hint()
    }
}

impl<Delays, Random> FusedIterator for Jitter<Delays, Random>
where
    Delays: FusedIterator<Item = Duration>,
    Random: FnMut() -> f64,
{
}

impl<Delays: ExplicitLimit<Item = Duration>, Random: FnMut() -> f64> ExplicitLimit
    for Jitter<Delays, Random>
{
}

/// `delay · factor`: zero for a zero delay or a factor that is negative or NaN, at most `delay`
/// for a factor of at most 1, and at most [`Duration::MAX`].
fn scale(delay: Duration, factor: f64) -> Duration {
    // A zero delay stays zero, even for an infinite factor, whose product with zero is NaN.
    if delay.is_zero() || factor.is_nan() || factor <= 0.0 {
        return Duration::ZERO;
    }
    let scaled = Duration::try_from_secs_f64(delay.as_secs_f64() * factor).unwrap_or(Duration::MAX);
    // An `f64` holds a long delay in seconds with less than nanosecond precision, so the product
    // may round up beyond the delay.
    if factor <= 1.0 {
        scaled.min(delay)
    } else {
        scaled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use crate::delay::limit::limited;
    use crate::delay::{Exponential, Fixed};
    use assertr::prelude::*;

    #[test]
    fn take_total_ends_before_the_delay_that_exceeds_the_budget() {
        let delays = limited(Exponential::of(1.seconds()).take_total(10.seconds()));

        assert_that!(delays).contains_exactly([1.seconds(), 2.seconds(), 4.seconds()]);
    }

    #[test]
    fn take_total_may_use_up_the_budget_exactly() {
        let delays = limited([4.seconds(), 6.seconds(), 1.milliseconds()].take_total(10.seconds()));

        assert_that!(delays).contains_exactly([4.seconds(), 6.seconds()]);
    }

    #[test]
    fn take_total_ends_for_good_at_the_first_delay_beyond_the_budget() {
        // The shorter delay after the long one would fit, but the delays already ended.
        let delays = limited([1.seconds(), 5.seconds(), 1.seconds()].take_total(3.seconds()));

        assert_that!(delays).contains_exactly([1.seconds()]);
    }

    #[test]
    fn take_total_combines_with_take() {
        let by_count = limited(Fixed::of(1.seconds()).take(2).take_total(10.seconds()));
        let by_budget = limited(Fixed::of(1.seconds()).take(20).take_total(3.seconds()));

        assert_that!(by_count).contains_exactly([1.seconds(); 2]);
        assert_that!(by_budget).contains_exactly([1.seconds(); 3]);
    }

    #[test]
    fn take_total_ends_with_the_delays() {
        let mut delays = [1.seconds()].take_total(10.seconds());

        assert_that!(delays.next()).is_equal_to(Some(1.seconds()));
        assert_that!(delays.next()).is_none();
        assert_that!(delays.next()).is_none();
    }

    #[test]
    fn take_total_hints_at_no_delays_once_it_ended() {
        let mut delays = [1.seconds(), 5.seconds()].take_total(3.seconds());

        let before = delays.size_hint();
        let taken: Vec<Duration> = delays.by_ref().collect();

        assert_that!(before).is_equal_to((0, Some(2)));
        assert_that!(taken).is_equal_to(vec![1.seconds()]);
        assert_that!(delays.size_hint()).is_equal_to((0, Some(0)));
    }

    #[test]
    fn jitter_shows_its_delays_in_debug() {
        let delays = [1.seconds()].jitter(|| 0.5);

        assert_that!(format!("{delays:?}")).is_equal_to("Jitter { delays: IntoIter([1s]), .. }");
    }

    #[test]
    fn forever_passes_the_delays_through() {
        let delays: Vec<Duration> = Exponential::of(1.seconds())
            .capped_at(2.seconds())
            .forever()
            .take(3)
            .collect();

        assert_that!(delays).contains_exactly([1.seconds(), 2.seconds(), 2.seconds()]);
    }

    #[test]
    fn multiplies_each_delay_by_a_random_factor() {
        // Arrange
        let mut factors = [0.5, 1.0, 1.5].into_iter();

        // Act
        let delays = limited(
            Fixed::of(1.seconds())
                .take(3)
                .jitter(move || factors.next().unwrap_or(1.0)),
        );

        // Assert
        assert_that!(delays).contains_exactly([
            500.milliseconds(),
            1.seconds(),
            1500.milliseconds(),
        ]);
    }

    #[test]
    fn a_negative_or_nan_factor_gives_no_delay() {
        let mut factors = [-1.0, f64::NAN].into_iter();

        let delays = limited(
            Fixed::of(1.seconds())
                .take(2)
                .jitter(move || factors.next().unwrap_or(1.0)),
        );

        assert_that!(delays).contains_exactly([Duration::ZERO, Duration::ZERO]);
    }

    #[test]
    fn a_zero_delay_stays_zero() {
        let mut factors = [0.5, f64::INFINITY].into_iter();

        let delays = limited(
            Fixed::of(Duration::ZERO)
                .take(2)
                .jitter(move || factors.next().unwrap_or(1.0)),
        );

        assert_that!(delays).contains_exactly([Duration::ZERO, Duration::ZERO]);
    }

    #[test]
    fn a_factor_of_at_most_one_never_lengthens_a_delay() {
        // About 200 days, more than an `f64` holds in seconds with nanosecond precision.
        let delay = Duration::new(17_280_000, 999_999_999);

        let delays = limited(Fixed::of(delay).take(1).jitter(|| 1.0));

        assert_that!(delays).contains_exactly([delay]);
    }

    #[test]
    fn saturates_at_the_maximum_duration() {
        let delays = limited(Fixed::of(Duration::MAX).take(1).jitter(|| 2.0));

        assert_that!(delays).contains_exactly([Duration::MAX]);
    }

    #[test]
    fn keeps_an_explicit_lack_of_a_limit() {
        let delays = limited(Fixed::of(2.seconds()).forever().jitter(|| 0.25).take(2));

        assert_that!(delays).contains_exactly([500.milliseconds(), 500.milliseconds()]);
    }
}
