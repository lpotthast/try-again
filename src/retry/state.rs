//! The running state of a retry, which blocking and async retries share: the remaining delays,
//! the closures that handle each failure, what the retry took so far, and the decision after each
//! failure.

use super::closures::{OverrideDelay, RetryHook, WorthAnotherTry};
use crate::{RetryError, StopReason};
use core::time::Duration;

/// What a retry does after a failure.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Next {
    /// Try again after this delay.
    Retry(Duration),
    /// Give up for this reason.
    Stop(StopReason),
}

/// The running state of a retry: its remaining delays, the closures that handle its failures, its
/// deadline, and what the retry took so far.
pub(super) struct RetryState<Delays, When, OverrideNextDelay, OnRetry> {
    delays: Delays,
    when: When,
    override_next_delay: OverrideNextDelay,
    on_retry: OnRetry,
    /// The deadline of an async retry, measured from the start of its first try.
    deadline: Option<Duration>,
    tries: u32,
    total_delay: Duration,
}

impl<Delays, When, OverrideNextDelay, OnRetry> RetryState<Delays, When, OverrideNextDelay, OnRetry>
where
    Delays: Iterator<Item = Duration>,
{
    pub(super) fn new(
        delays: Delays,
        when: When,
        override_next_delay: OverrideNextDelay,
        on_retry: OnRetry,
        deadline: Option<Duration>,
    ) -> Self {
        Self {
            delays,
            when,
            override_next_delay,
            on_retry,
            deadline,
            tries: 0,
            total_delay: Duration::ZERO,
        }
    }

    /// Decides what to do after the failure of a try.
    ///
    /// The predicate sees the failure before the retry takes a delay, so a rejected failure uses
    /// up none. The override then chooses the delay from the scheduled one. If the deadline would
    /// expire before that delay ends, the retry gives up right away, without waiting for it.
    /// Otherwise the hook sees the chosen delay.
    pub(super) fn after<Failure>(&mut self, failure: &Failure) -> Next
    where
        When: WorthAnotherTry<Failure>,
        OverrideNextDelay: OverrideDelay<Failure>,
        OnRetry: RetryHook<Failure>,
    {
        if !self.count_and_judge(failure) {
            return Next::Stop(StopReason::Rejected);
        }
        let Some(scheduled) = self.delays.next() else {
            return Next::Stop(StopReason::Exhausted);
        };
        let delay = self.override_next_delay.next_delay(failure, scheduled);
        if self.deadline_ends_before(delay) {
            return Next::Stop(StopReason::Expired);
        }
        self.on_retry.on_retry(failure, delay, self.tries);
        Next::Retry(delay)
    }

    /// Decides why the retry gives up after a failure that arrived just as the deadline expired.
    ///
    /// The predicate still sees the failure. The reason then does not depend on whether the
    /// failure came just before the deadline or at it. No retry follows, so the retry takes no
    /// delay, and neither the override nor the hook runs.
    pub(super) fn after_expiry<Failure>(&mut self, failure: &Failure) -> StopReason
    where
        When: WorthAnotherTry<Failure>,
    {
        if self.count_and_judge(failure) {
            StopReason::Expired
        } else {
            StopReason::Rejected
        }
    }

    /// Counts the try that failed with `failure`, and tells whether the failure is worth another
    /// try.
    fn count_and_judge<Failure>(&mut self, failure: &Failure) -> bool
    where
        When: WorthAnotherTry<Failure>,
    {
        self.count_try();
        self.when.is_worth_another_try(failure)
    }

    /// Whether the deadline certainly expires before `delay` ends.
    ///
    /// The retry reads no clock, but the time since the first try started is at least the sum of
    /// the delays that it waited in full. So when that sum plus `delay` reaches the deadline, the
    /// deadline expires during the delay, or as it ends, and no time is left for another try.
    fn deadline_ends_before(&self, delay: Duration) -> bool {
        self.deadline
            .is_some_and(|deadline| self.total_delay.saturating_add(delay) >= deadline)
    }

    /// Counts a try. [`after`](Self::after) and [`after_expiry`](Self::after_expiry) count the
    /// tries that they judge, so callers count only a try that the deadline canceled.
    pub(super) fn count_try(&mut self) {
        self.tries = self.tries.saturating_add(1);
    }

    /// Counts a delay that the retry waited in full.
    pub(super) fn waited(&mut self, delay: Duration) {
        self.total_delay = self.total_delay.saturating_add(delay);
    }

    /// The error of a retry that gave up with `failure`.
    pub(super) fn gave_up<F>(&self, failure: F, reason: StopReason) -> RetryError<F> {
        RetryError {
            failure,
            reason,
            tries: self.tries,
            total_delay: self.total_delay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use assertr::prelude::*;
    use std::cell::RefCell;
    use std::vec::Vec;

    /// A call of one of the closures of a [`recording`] state, with the delay that it received.
    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        When,
        OverrideNextDelay(Duration),
        OnRetry(Duration),
    }

    type Calls = RefCell<Vec<Call>>;

    /// A state whose closures record their calls in the order in which they run. Its predicate
    /// accepts every failure if `accept` is set, and its override doubles every delay.
    #[expect(clippy::type_complexity, reason = "closures have no names")]
    fn recording<'c>(
        calls: &'c Calls,
        delays: impl IntoIterator<Item = Duration, IntoIter: 'c>,
        accept: bool,
        deadline: Option<Duration>,
    ) -> RetryState<
        impl Iterator<Item = Duration> + 'c,
        impl FnMut(&()) -> bool + 'c,
        impl FnMut(&(), Duration) -> Duration + 'c,
        impl FnMut(&(), Duration, u32) + 'c,
    > {
        RetryState::new(
            delays.into_iter(),
            move |&(): &()| {
                calls.borrow_mut().push(Call::When);
                accept
            },
            move |&(): &(), delay: Duration| {
                calls.borrow_mut().push(Call::OverrideNextDelay(delay));
                delay * 2
            },
            move |&(): &(), delay: Duration, _tries: u32| {
                calls.borrow_mut().push(Call::OnRetry(delay));
            },
            deadline,
        )
    }

    #[test]
    fn runs_the_predicate_then_the_override_then_the_hook() {
        let calls = Calls::default();
        let mut state = recording(&calls, [1.seconds()], true, None);

        let next = state.after(&());

        // The override receives the scheduled delay, and the hook the overridden one.
        assert_that!(next).is_equal_to(Next::Retry(2.seconds()));
        assert_that!(calls.take()).contains_exactly([
            Call::When,
            Call::OverrideNextDelay(1.seconds()),
            Call::OnRetry(2.seconds()),
        ]);
    }

    #[test]
    fn a_rejected_failure_takes_no_delay() {
        let calls = Calls::default();
        let mut delays = [1.seconds(), 2.seconds()].into_iter();
        let mut state = recording(&calls, &mut delays, false, None);

        let next = state.after(&());

        drop(state);
        assert_that!(next).is_equal_to(Next::Stop(StopReason::Rejected));
        assert_that!(calls.take()).contains_exactly([Call::When]);
        assert_that!(delays.next()).is_equal_to(Some(1.seconds()));
    }

    #[test]
    fn without_delays_left_runs_neither_the_override_nor_the_hook() {
        let calls = Calls::default();
        let mut state = recording(&calls, [], true, None);

        let next = state.after(&());

        assert_that!(next).is_equal_to(Next::Stop(StopReason::Exhausted));
        assert_that!(calls.take()).contains_exactly([Call::When]);
    }

    #[test]
    fn gives_up_before_a_delay_that_the_deadline_would_end() {
        // Arrange
        let calls = Calls::default();
        let mut state = recording(&calls, [1.seconds(), 2.seconds()], true, Some(6.seconds()));

        // Act: 1 s, doubled to 2 s by the override, fits. The next 2 s, doubled to 4 s, would end
        // at the deadline.
        let first = state.after(&());
        state.waited(2.seconds());
        let second = state.after(&());

        // Assert
        assert_that!(first).is_equal_to(Next::Retry(2.seconds()));
        assert_that!(second).is_equal_to(Next::Stop(StopReason::Expired));
        assert_that!(calls.take()).contains_exactly([
            Call::When,
            Call::OverrideNextDelay(1.seconds()),
            Call::OnRetry(2.seconds()),
            Call::When,
            Call::OverrideNextDelay(2.seconds()),
        ]);
    }

    #[test]
    fn after_expiry_lets_only_the_predicate_see_the_failure_and_takes_no_delay() {
        let calls = Calls::default();
        let mut delays = [1.seconds()].into_iter();
        let mut state = recording(&calls, &mut delays, true, Some(1.seconds()));

        let reason = state.after_expiry(&());

        drop(state);
        assert_that!(reason).is_equal_to(StopReason::Expired);
        assert_that!(calls.take()).contains_exactly([Call::When]);
        assert_that!(delays.next()).is_equal_to(Some(1.seconds()));
    }

    #[test]
    fn after_expiry_lets_the_predicate_reject_the_failure() {
        let calls = Calls::default();
        let mut state = recording(&calls, [1.seconds()], false, Some(1.seconds()));

        let reason = state.after_expiry(&());

        assert_that!(reason).is_equal_to(StopReason::Rejected);
    }

    #[test]
    fn passes_the_number_of_tries_to_the_hook() {
        let tries = RefCell::new(Vec::new());
        let mut state = RetryState::new(
            [1.seconds(), 1.seconds()].into_iter(),
            (),
            (),
            |&(): &(), _delay: Duration, try_number: u32| tries.borrow_mut().push(try_number),
            None,
        );

        state.after(&());
        state.after(&());
        state.after(&());

        drop(state);
        assert_that!(tries.into_inner()).contains_exactly([1, 2]);
    }

    #[test]
    fn counts_tries_and_waited_delays() {
        let mut state = RetryState::new([1.seconds(), 2.seconds()].into_iter(), (), (), (), None);

        state.after(&());
        state.waited(1.seconds());
        state.count_try();

        let error = state.gave_up((), StopReason::Expired);
        assert_that!(error.tries).is_equal_to(2);
        assert_that!(error.total_delay).is_equal_to(1.seconds());
    }
}
