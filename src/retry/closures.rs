//! The closures that handle the failures of a retry: one sealed trait each for what `when`,
//! `override_next_delay` and `on_retry` set. A closure implements its trait, and so does `()`,
//! which stands for a closure that is not set.
//!
//! The traits are public but unnameable, which keeps them sealed.

use core::time::Duration;

/// Decides whether a failure is worth another try: the predicate passed to `when`, or `()`, which
/// accepts every failure.
pub trait WorthAnotherTry<Failure> {
    fn is_worth_another_try(&mut self, failure: &Failure) -> bool;
}

impl<Failure, F: FnMut(&Failure) -> bool> WorthAnotherTry<Failure> for F {
    fn is_worth_another_try(&mut self, failure: &Failure) -> bool {
        self(failure)
    }
}

impl<Failure> WorthAnotherTry<Failure> for () {
    fn is_worth_another_try(&mut self, _failure: &Failure) -> bool {
        true
    }
}

/// Chooses the delay after a failure from the scheduled one: the closure passed to
/// `override_next_delay`, or `()`, which keeps the scheduled delay.
pub trait OverrideDelay<Failure> {
    fn next_delay(&mut self, failure: &Failure, scheduled: Duration) -> Duration;
}

impl<Failure, F: FnMut(&Failure, Duration) -> Duration> OverrideDelay<Failure> for F {
    fn next_delay(&mut self, failure: &Failure, scheduled: Duration) -> Duration {
        self(failure, scheduled)
    }
}

impl<Failure> OverrideDelay<Failure> for () {
    fn next_delay(&mut self, _failure: &Failure, scheduled: Duration) -> Duration {
        scheduled
    }
}

/// Observes each failure before the retry waits for the next delay: the hook passed to
/// `on_retry`, or `()`, which does nothing.
pub trait RetryHook<Failure> {
    fn on_retry(&mut self, failure: &Failure, delay: Duration, tries: u32);
}

impl<Failure, F: FnMut(&Failure, Duration, u32)> RetryHook<Failure> for F {
    fn on_retry(&mut self, failure: &Failure, delay: Duration, tries: u32) {
        self(failure, delay, tries);
    }
}

impl<Failure> RetryHook<Failure> for () {
    fn on_retry(&mut self, _failure: &Failure, _delay: Duration, _tries: u32) {}
}

/// Implemented only while none of the three closures is set. `timeout_each` requires it: it
/// changes the failure that the closures receive, and so must be called before them.
#[diagnostic::on_unimplemented(
    message = "`timeout_each` must come before `when`, `override_next_delay` and `on_retry`",
    label = "`timeout_each` called after a closure that handles failures",
    note = "`timeout_each` changes the failure that those closures receive to a `TryFailure`"
)]
pub trait NoClosuresYet {}

impl NoClosuresYet for ((), (), ()) {}
