//! [`ExplicitLimit`], the limit that the delays of a retry must state.

/// Marks iterators of delays that state their limit explicitly.
///
/// A retry accepts only such delays. It tries again once per delay, so infinite delays would make
/// it retry forever by accident. A retry therefore requires its delays to state their limit, even
/// when it has a deadline:
///
/// - `.take(n)`: at most `n` retries. [`Fixed`](super::Fixed) and
///   [`Exponential`](super::Exponential) have `take` as a method, and an iterator has its own.
/// - `.take_total(budget)`: delays while their sum stays within `budget`. `Fixed` and
///   `Exponential` have it as a method, and [`DelaysExt`](super::DelaysExt) adds it to any
///   iterator.
/// - `.forever()`: explicitly no limit of their own. `Fixed` and `Exponential` have `forever` as
///   a method, and [`DelaysExt`](super::DelaysExt) adds it to any iterator. With infinite
///   delays, the retry then ends only on success, when `when` rejects a failure, at its
///   deadline, or when its future is dropped. Nothing can stop a blocking retry from the outside,
///   so it blocks its thread until one of the first two happens.
///
/// Finite sources state their limit by their length: arrays, `Vec`s and `VecDeque`s (feature
/// `alloc`), `Option`s, the ranges `a..b` and `a..=b`, `iter::once`, `iter::empty` and
/// `iter::repeat_n`. The standard library's adapters, such as `map`, `chain`, `filter`, `scan` and
/// `copied`, and this crate's [`jitter`](super::DelaysExt::jitter), keep the limit of the delays
/// they adapt, so you can shape delays after limiting them.
///
/// Implement `ExplicitLimit` for a finite iterator of your own. You cannot implement it for a
/// finite iterator from another crate; wrap that one in `.forever()`, which accepts any iterator
/// and still ends when the iterator does.
///
/// A borrowed iterator, `.delays(&mut delays)`, shares its limit between retries. A retry takes a
/// delay from it only when it needs one: not after a failure that `when` rejects, nor after one
/// that arrives as the deadline expires. To find out whether the next delay would end after its
/// deadline, though, a retry takes that delay, and then gives up without waiting for it.
///
/// ```compile_fail,E0277
/// use try_again::{retry, DurationExt};
///
/// // Error: `repeat` never ends, and states no limit.
/// let _ = retry(|| Err::<(), ()>(())).delays(std::iter::repeat(1.seconds()));
/// ```
#[diagnostic::on_unimplemented(
    message = "the delays `{Self}` state no limit",
    label = "delays without a stated limit",
    note = "limit the delays with `.take(n)` or `.take_total(budget)`, or opt out of a limit \
            with `.forever()`; `try_again::delay::DelaysExt` adds the last two to iterators",
    note = "a `Vec`, a `VecDeque` or a `Box<dyn ExplicitLimit<..>>` states its limit only with \
            the feature `alloc`"
)]
pub trait ExplicitLimit: Iterator {}

mod core_impls {
    use super::ExplicitLimit;
    use core::iter;
    use core::ops::{Range, RangeInclusive};

    // Finite iterators.
    impl<I: Iterator> ExplicitLimit for iter::Take<I> {}
    impl<T: Clone> ExplicitLimit for iter::RepeatN<T> {}
    impl<T, const N: usize> ExplicitLimit for core::array::IntoIter<T, N> {}
    impl<T> ExplicitLimit for core::slice::Iter<'_, T> {}
    impl<T> ExplicitLimit for core::option::IntoIter<T> {}
    impl<T> ExplicitLimit for iter::Empty<T> {}
    impl<T> ExplicitLimit for iter::Once<T> {}
    // Only the ranges with an end. `a..` has none, and so states no limit.
    impl<T> ExplicitLimit for Range<T> where Range<T>: Iterator {}
    impl<T> ExplicitLimit for RangeInclusive<T> where RangeInclusive<T>: Iterator {}

    // Adapters that end when the iterator they adapt ends.
    impl<'a, T: Copy + 'a, I: ExplicitLimit<Item = &'a T>> ExplicitLimit for iter::Copied<I> {}
    impl<'a, T: Clone + 'a, I: ExplicitLimit<Item = &'a T>> ExplicitLimit for iter::Cloned<I> {}
    impl<B, I: ExplicitLimit, F: FnMut(I::Item) -> B> ExplicitLimit for iter::Map<I, F> {}
    impl<I: ExplicitLimit, F: FnMut(&I::Item)> ExplicitLimit for iter::Inspect<I, F> {}
    impl<I: ExplicitLimit, P: FnMut(&I::Item) -> bool> ExplicitLimit for iter::Filter<I, P> {}
    impl<B, I, F> ExplicitLimit for iter::FilterMap<I, F>
    where
        I: ExplicitLimit,
        F: FnMut(I::Item) -> Option<B>,
    {
    }
    impl<B, I, P> ExplicitLimit for iter::MapWhile<I, P>
    where
        I: ExplicitLimit,
        P: FnMut(I::Item) -> Option<B>,
    {
    }
    impl<I: ExplicitLimit, P: FnMut(&I::Item) -> bool> ExplicitLimit for iter::TakeWhile<I, P> {}
    impl<B, I, St, F> ExplicitLimit for iter::Scan<I, St, F>
    where
        I: ExplicitLimit,
        F: FnMut(&mut St, I::Item) -> Option<B>,
    {
    }
    impl<I: ExplicitLimit, P: FnMut(&I::Item) -> bool> ExplicitLimit for iter::SkipWhile<I, P> {}
    impl<I: ExplicitLimit> ExplicitLimit for iter::Skip<I> {}
    impl<I: ExplicitLimit> ExplicitLimit for iter::StepBy<I> {}
    impl<I: ExplicitLimit> ExplicitLimit for iter::Enumerate<I> {}
    impl<I: ExplicitLimit> ExplicitLimit for iter::Peekable<I> {}
    impl<I: ExplicitLimit> ExplicitLimit for iter::Fuse<I> {}
    impl<I: ExplicitLimit + DoubleEndedIterator> ExplicitLimit for iter::Rev<I> {}
    impl<A: ExplicitLimit, B: ExplicitLimit<Item = A::Item>> ExplicitLimit for iter::Chain<A, B> {}
    impl<A: ExplicitLimit, B: Iterator> ExplicitLimit for iter::Zip<A, B> {}
    impl<I: ExplicitLimit + ?Sized> ExplicitLimit for &mut I {}
}

/// The collections of `alloc`, which state their limit by their length, and boxed delays.
#[cfg(feature = "alloc")]
mod alloc_impls {
    use super::ExplicitLimit;
    use alloc::boxed::Box;
    use alloc::collections::vec_deque;

    impl<T> ExplicitLimit for alloc::vec::IntoIter<T> {}
    impl<T> ExplicitLimit for vec_deque::IntoIter<T> {}
    impl<I: ExplicitLimit + ?Sized> ExplicitLimit for Box<I> {}
}

/// Collects delays that state their limit, as a retry accepts them.
#[cfg(test)]
pub(super) fn limited<D>(delays: D) -> Vec<core::time::Duration>
where
    D: IntoIterator<Item = core::time::Duration>,
    D::IntoIter: ExplicitLimit,
{
    delays.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use crate::delay::{DelaysExt, Exponential, Fixed};
    use assertr::prelude::*;
    use core::time::Duration;

    #[test]
    fn adapters_of_limited_delays_stay_limited() {
        let strategy = Exponential::of(1.seconds()).capped_at(4.seconds());

        let delays = limited(
            std::iter::once(Duration::ZERO)
                .chain(strategy.take(3).map(|delay| delay * 2))
                .filter(|delay| *delay < 8.seconds()),
        );

        assert_that!(delays).contains_exactly([Duration::ZERO, 2.seconds(), 4.seconds()]);
    }

    #[test]
    fn lists_state_their_limit() {
        let list = [1.seconds(), 2.seconds()];

        assert_that!(limited(list)).contains_exactly([1.seconds(), 2.seconds()]);
        #[cfg(feature = "alloc")]
        assert_that!(limited(vec![3.seconds()])).contains_exactly([3.seconds()]);
        assert_that!(limited(list.iter().copied())).contains_exactly([1.seconds(), 2.seconds()]);
        assert_that!(limited(std::iter::repeat_n(5.milliseconds(), 2)))
            .contains_exactly([5.milliseconds(), 5.milliseconds()]);
    }

    /// The lengths in seconds of delays that state their limit.
    fn seconds<D>(delays: D) -> Vec<u64>
    where
        D: IntoIterator<Item = Duration>,
        D::IntoIter: ExplicitLimit,
    {
        limited(delays).iter().map(Duration::as_secs).collect()
    }

    #[test]
    #[expect(
        clippy::cloned_instead_of_copied,
        reason = "`cloned` keeps the limit too"
    )]
    fn every_documented_source_and_adapter_states_its_limit() {
        use std::iter;

        let list = [1.seconds(), 2.seconds(), 3.seconds()];
        let delays = || list.into_iter();

        // Finite sources.
        assert_that!(seconds(list)).contains_exactly([1, 2, 3]);
        assert_that!(seconds(list.iter().copied())).contains_exactly([1, 2, 3]);
        assert_that!(seconds(list.iter().cloned())).contains_exactly([1, 2, 3]);
        assert_that!(seconds(Some(1.seconds()))).contains_exactly([1]);
        assert_that!(seconds(iter::once(1.seconds()))).contains_exactly([1]);
        assert_that!(seconds(iter::empty())).is_empty();
        assert_that!(seconds(iter::repeat_n(1.seconds(), 2))).contains_exactly([1, 1]);
        assert_that!(seconds((1..4).map(DurationExt::seconds))).contains_exactly([1, 2, 3]);
        assert_that!(seconds((1..=3).map(DurationExt::seconds))).contains_exactly([1, 2, 3]);

        // Adapters of delays that state their limit.
        let two = 2.seconds();
        assert_that!(seconds(delays().map(|delay| delay * 2))).contains_exactly([2, 4, 6]);
        assert_that!(seconds(delays().inspect(|_delay| {}))).contains_exactly([1, 2, 3]);
        assert_that!(seconds(delays().filter(|delay| *delay != two))).contains_exactly([1, 3]);
        assert_that!(seconds(delays().filter_map(|delay| delay.checked_sub(two))))
            .contains_exactly([0, 1]);
        assert_that!(seconds(delays().map_while(|delay| two.checked_sub(delay))))
            .contains_exactly([1, 0]);
        assert_that!(seconds(delays().take_while(|delay| *delay < two))).contains_exactly([1]);
        assert_that!(seconds(delays().skip_while(|delay| *delay < two))).contains_exactly([2, 3]);
        assert_that!(seconds(delays().skip(1))).contains_exactly([2, 3]);
        assert_that!(seconds(delays().step_by(2))).contains_exactly([1, 3]);
        assert_that!(seconds(
            delays()
                .enumerate()
                .map(|(index, delay)| if index == 0 { two } else { delay })
        ))
        .contains_exactly([2, 2, 3]);
        assert_that!(seconds(delays().peekable())).contains_exactly([1, 2, 3]);
        assert_that!(seconds(delays().fuse())).contains_exactly([1, 2, 3]);
        assert_that!(seconds(delays().rev())).contains_exactly([3, 2, 1]);
        assert_that!(seconds(delays().chain(delays()))).contains_exactly([1, 2, 3, 1, 2, 3]);
        // The delays limit a `zip`, whatever it zips them with.
        assert_that!(seconds(delays().zip(1..).map(|(delay, n)| delay * n)))
            .contains_exactly([1, 4, 9]);
        assert_that!(seconds(delays().scan(Duration::ZERO, |sum, delay| {
            *sum += delay;
            Some(*sum)
        })))
        .contains_exactly([1, 3, 6]);
        let mut borrowed = delays();
        assert_that!(seconds(&mut borrowed)).contains_exactly([1, 2, 3]);
        assert_that!(borrowed.next()).is_equal_to(None);

        // The adapters of `DelaysExt`.
        assert_that!(seconds(list.take_total(3.seconds()))).contains_exactly([1, 2]);
        assert_that!(seconds(list.forever())).contains_exactly([1, 2, 3]);

        #[cfg(feature = "alloc")]
        {
            use std::collections::VecDeque;

            // A boxed iterator states its limit as a `dyn ExplicitLimit`. A `dyn Iterator` states
            // none.
            let boxed: Box<dyn ExplicitLimit<Item = Duration>> = Box::new(delays());

            assert_that!(seconds(Vec::from(list))).contains_exactly([1, 2, 3]);
            assert_that!(seconds(VecDeque::from(list))).contains_exactly([1, 2, 3]);
            assert_that!(seconds(boxed)).contains_exactly([1, 2, 3]);
        }
    }

    #[test]
    fn a_custom_strategy_is_a_plain_iterator() {
        // Arrange
        let linear = std::iter::successors(Some(1.seconds()), |delay| Some(*delay + 1.seconds()));

        // Act
        let delays = limited(linear.clone().take(3));
        let forever: Vec<Duration> = linear.forever().take(3).collect();

        // Assert
        assert_that!(delays).contains_exactly([1.seconds(), 2.seconds(), 3.seconds()]);
        assert_that!(forever).contains_exactly([1.seconds(), 2.seconds(), 3.seconds()]);
    }

    #[test]
    fn a_borrowed_iterator_shares_its_limit() {
        let mut budget = Fixed::of(1.seconds()).take(3);

        let first = limited(&mut budget).len();
        let second = limited(&mut budget).len();

        assert_that!(first).is_equal_to(3);
        assert_that!(second).is_equal_to(0);
    }
}
