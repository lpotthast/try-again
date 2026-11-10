//! The deadline of an async retry, from `timeout_total`: how a retry with a deadline runs.

use super::super::closures::{OverrideDelay, RetryHook, WorthAnotherTry};
use super::super::config::HasDelays;
use super::super::state::{Next, RetryState};
use super::super::{Retry, RetryConfig};
use super::race::{Raced, has_expired, race};
use super::{AsyncOperation, TryTimeout, yield_now};
use crate::sleep::{AsyncSleep, IntoAsyncSleep};
use crate::{RetryError, StopReason, TryFailure};
use core::pin::pin;
use core::time::Duration;

impl<Op, Delays, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>
    Retry<
        Op,
        RetryConfig<Delays, Duration, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>,
    >
where
    Op: AsyncOperation,
    TimeoutEach: TryTimeout,
    When: WorthAnotherTry<TimeoutEach::Failure<Op::Failure>>,
    OverrideNextDelay: OverrideDelay<TimeoutEach::Failure<Op::Failure>>,
    OnRetry: RetryHook<TimeoutEach::Failure<Op::Failure>>,
{
    /// Runs an async retry with a deadline.
    ///
    /// Between tries it waits with the sleep that [`sleep_with`](Retry::sleep_with) set, or with
    /// tokio if none was set. The sleep measures the deadline too.
    ///
    /// # Errors
    ///
    /// Returns a [`RetryError`] with a [`TryFailure`] if the retry gave up. Its failure is that of
    /// the last try that completed, including a try that failed just as the deadline expired, or
    /// [`TryFailure::TimedOut`] if the deadline canceled the first try. Its reason is
    /// [`StopReason::Expired`] if the deadline left no time for another try.
    ///
    /// # Panics
    ///
    /// With tokio's sleep, the default, it panics when it is first polled outside a tokio runtime,
    /// or in a runtime without the time driver; see [`TokioSleep`](crate::sleep::TokioSleep).
    pub async fn run(self) -> Result<Op::Success, RetryError<TryFailure<Op::Failure>>>
    where
        Delays: HasDelays,
        SleepWith: IntoAsyncSleep,
    {
        let Self {
            mut operation,
            settings,
        } = self;
        let sleep = settings.sleep_with.into_sleep();
        let deadline = settings.timeout_total;
        let mut state = RetryState::new(
            settings.delays,
            settings.when,
            settings.override_next_delay,
            settings.on_retry,
            Some(deadline),
        );
        let mut expiry = pin!(sleep.sleep(deadline));
        // No try has completed yet: if the deadline cancels the first one, this is the failure.
        let mut last_failure = TryFailure::TimedOut;
        let reason = loop {
            // The scope drops the output of the try before the delay, to keep the retry's future
            // `Send` even when the operation's success is not.
            let delay = {
                let Raced { output, expired } = race(
                    || settings.timeout_each.run_try(&mut operation, &sleep),
                    expiry.as_mut(),
                )
                .await;
                // A try that completes as the deadline expires still counts.
                let Some(out) = output else {
                    // The canceled try counts, but leaves no failure, so the earlier one is the
                    // last.
                    state.count_try();
                    break StopReason::Expired;
                };
                let failure = match out {
                    Ok(success) => return Ok(success),
                    Err(failure) => failure,
                };
                let next = if expired {
                    Next::Stop(state.after_expiry(&failure))
                } else {
                    state.after(&failure)
                };
                last_failure = TimeoutEach::into_try_failure(failure);
                match next {
                    Next::Retry(delay) => delay,
                    Next::Stop(reason) => break reason,
                }
            };
            let Raced { output, expired } = race(|| sleep.sleep(delay), expiry.as_mut()).await;
            if output.is_some() {
                state.waited(delay);
            }
            // A delay that ends as the deadline expires leaves no time for another try.
            if expired {
                break StopReason::Expired;
            }
            yield_now().await;
            // The deadline may expire while the executor runs other tasks. A try must not start
            // after it.
            if has_expired(expiry.as_mut()).await {
                break StopReason::Expired;
            }
        };
        Err(state.gave_up(last_failure, reason))
    }
}
