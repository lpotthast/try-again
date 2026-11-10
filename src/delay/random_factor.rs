//! [`random_factor`], random factors from fastrand for [`jitter`](super::DelaysExt::jitter).

/// Returns a random factor in `0.0..1.0`, for "full jitter" with
/// [`DelaysExt::jitter`](super::DelaysExt::jitter): `.jitter(delay::random_factor)`.
///
/// It comes from fastrand's generator for the current thread. That generator is fast, but not
/// cryptographically secure; jitter does not need it to be. fastrand seeds each thread's generator
/// once, from the clock and the thread. In the browser (`wasm32-unknown-unknown`) it seeds from
/// `crypto.getRandomValues`, which this crate's feature `fastrand` turns on there. So different
/// clients get different factors.
///
/// See [`jitter`](super::DelaysExt::jitter) for equal jitter, and for reproducible delays in
/// tests.
///
/// ```
/// use try_again::delay::{random_factor, DelaysExt, Exponential};
/// use try_again::{retry, DurationExt};
///
/// let strategy = Exponential::of(100.milliseconds()).capped_at(5.seconds());
/// let out = retry(|| Ok::<(), ()>(()))
///     .delays(strategy.take(5).jitter(random_factor))
///     .run();
/// ```
#[must_use]
pub fn random_factor() -> f64 {
    fastrand::f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use std::collections::HashSet;

    #[test]
    fn stays_within_zero_and_one() {
        for _ in 0..1_000 {
            let factor = random_factor();

            assert_that!((0.0..1.0).contains(&factor)).is_true();
        }
    }

    #[test]
    fn differs_between_calls() {
        let factors: HashSet<u64> = (0..100).map(|_| random_factor().to_bits()).collect();

        assert_that!(factors.len()).is_greater_than(90);
    }

    #[test]
    fn differs_between_threads() {
        let first: Vec<u64> = (0..10).map(|_| random_factor().to_bits()).collect();
        let other = std::thread::spawn(|| {
            (0..10)
                .map(|_| random_factor().to_bits())
                .collect::<Vec<_>>()
        })
        .join()
        .expect("the thread must not panic");

        assert_that!(first).is_not_equal_to(other);
    }
}
