//! The blocking retry: [`retry`], and how it runs.

use super::RetryConfig;
use super::builder::{AcceptsSleep, AcceptsTimeLimits, Operation, Retry};
use super::closures::{OverrideDelay, RetryHook, WorthAnotherTry};
use super::config::HasDelays;
use super::state::{Next, RetryState};
use crate::sleep::{IntoSleep, Sleep};
use crate::{Fallible, RetryError};

/// Builds a retry of a blocking operation, a closure that returns a [`Fallible`] such as a
/// `Result` or an `Option`.
///
/// Set its delays with [`delays`](Retry::delays), and run it with `run()`; see [`Retry`] for
/// all settings.
///
/// ```
/// # #[cfg(feature = "std")] {
/// use try_again::delay::Exponential;
/// use try_again::{retry, DurationExt};
///
/// let mut tries = 0;
/// let out = retry(|| {
///     tries += 1;
///     if tries < 3 { Err("not yet") } else { Ok(tries) }
/// })
/// .delays(Exponential::of(1.milliseconds()).capped_at(10.milliseconds()).take(5))
/// .run();
///
/// assert_eq!(out, Ok(3));
///
/// let error = retry(|| Err::<(), _>("unavailable"))
///     .delays(Exponential::of(1.milliseconds()).take(2))
///     .run()
///     .unwrap_err();
///
/// assert_eq!(error.failure, "unavailable");
/// assert_eq!(error.tries, 3);
/// assert_eq!(error.total_delay, 3.milliseconds());
/// # }
/// ```
///
/// A blocking try cannot be canceled, so a blocking retry has no time limits. Its delays can
/// still have one: [`take_total`](crate::delay::DelaysExt::take_total) limits their sum. A `when`
/// predicate can also enforce a time limit, with any clock you like:
///
/// ```
/// # #[cfg(feature = "std")] {
/// use std::time::Instant;
/// use try_again::delay::Fixed;
/// use try_again::{retry, DurationExt, StopReason};
///
/// let start = Instant::now();
/// let error = retry(|| Err::<(), _>("unavailable"))
///     // Retries for at most 20 ms, tries included.
///     .when(|_error| start.elapsed() < 20.milliseconds())
///     .delays(Fixed::of(5.milliseconds()).forever())
///     .run()
///     .unwrap_err();
///
/// assert_eq!(error.reason, StopReason::Rejected);
/// # }
/// ```
pub fn retry<Op, Out>(operation: Op) -> Retry<Blocking<Op>>
where
    Op: FnMut() -> Out,
    Out: Fallible,
{
    Retry::new(Blocking(operation))
}

/// The operation of a blocking retry, from [`retry`]. The type is public but unnameable.
pub struct Blocking<Op>(Op);

impl<Op, Out> Operation for Blocking<Op>
where
    Op: FnMut() -> Out,
    Out: Fallible,
{
    type Success = Out::Success;
    type Failure = Out::Failure;
}

impl<Op> AcceptsSleep<()> for Blocking<Op> {}

#[diagnostic::do_not_recommend]
impl<Op, S: Sleep> AcceptsSleep<S> for Blocking<Op> {}

impl<Op> AcceptsTimeLimits<(), ()> for Blocking<Op> {}

/// With a single implementation for a blocking operation, the compiler would infer the time limits
/// of `config` from it, and report a mismatch of types instead of the message of
/// [`AcceptsTimeLimits`]. A second one, for a type that no config has, prevents that.
enum NoTimeLimit {}

impl<Op> AcceptsTimeLimits<NoTimeLimit, NoTimeLimit> for Blocking<Op> {}

impl<Op, Out, Delays, When, OverrideNextDelay, OnRetry, SleepWith>
    Retry<Blocking<Op>, RetryConfig<Delays, (), (), When, OverrideNextDelay, OnRetry, SleepWith>>
where
    Op: FnMut() -> Out,
    Out: Fallible,
    When: WorthAnotherTry<Out::Failure>,
    OverrideNextDelay: OverrideDelay<Out::Failure>,
    OnRetry: RetryHook<Out::Failure>,
{
    /// Runs a blocking retry.
    ///
    /// Between tries it waits with the sleep that [`sleep_with`](Retry::sleep_with) set, or puts
    /// the thread to sleep if none was set.
    ///
    /// # Errors
    ///
    /// Returns a [`RetryError`] with the last failure if the retry gave up.
    pub fn run(self) -> Result<Out::Success, RetryError<Out::Failure>>
    where
        Delays: HasDelays,
        SleepWith: IntoSleep,
    {
        let Self {
            operation: Blocking(mut operation),
            settings,
        } = self;
        let sleep = settings.sleep_with.into_sleep();
        let mut state = RetryState::new(
            settings.delays,
            settings.when,
            settings.override_next_delay,
            settings.on_retry,
            None,
        );
        loop {
            // The scope drops a failure before the delay, so it holds no resources while the retry
            // waits.
            let delay = {
                let failure = match operation().into_result() {
                    Ok(success) => return Ok(success),
                    Err(failure) => failure,
                };
                match state.after(&failure) {
                    Next::Retry(delay) => delay,
                    Next::Stop(reason) => return Err(state.gave_up(failure, reason)),
                }
            };
            sleep.sleep(delay);
            state.waited(delay);
        }
    }
}
