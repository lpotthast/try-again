//! [`Retry`], the builder of a retry, and its settings.

use super::RetryConfig;
use super::asynchronous::{AsyncOperation, TryTimeout};
use super::closures::{NoClosuresYet, OverrideDelay, RetryHook, WorthAnotherTry};
use super::config::NoSettingsYet;
use crate::delay::ExplicitLimit;
use core::fmt::{self, Debug, Formatter};
use core::time::Duration;

/// The operation of a retry: a blocking one from [`retry()`](crate::retry()), or an async one
/// from [`retry_async()`](crate::retry_async()). The trait is public but unnameable, which keeps
/// it sealed.
pub trait Operation {
    /// The value of a successful try.
    type Success;
    /// The failure of a try.
    type Failure;
}

/// Implemented by the operations whose retry can wait with `Sleep`: a blocking operation accepts
/// a [`Sleep`](crate::sleep::Sleep), an async one accepts an
/// [`AsyncSleep`](crate::sleep::AsyncSleep), and both accept `()`, the default sleep. Sealed like
/// [`Operation`].
#[diagnostic::on_unimplemented(
    message = "the retry cannot sleep with `{Sleep}`",
    label = "a sleep of the wrong kind",
    note = "a blocking retry sleeps with a `try_again::sleep::Sleep`, such as a `Fn(Duration)` \
            closure",
    note = "an async retry sleeps with a `try_again::sleep::AsyncSleep`, such as a \
            `Fn(Duration) -> impl Future<Output = ()>` closure"
)]
pub trait AcceptsSleep<Sleep> {}

/// Implemented by the operations that accept the time limits `TimeoutTotal` and `TimeoutEach` of
/// a [`RetryConfig`]: an async operation accepts any, and a blocking one accepts none. Sealed like
/// [`Operation`].
#[diagnostic::on_unimplemented(
    message = "a blocking retry has no time limits",
    label = "the config has a time limit, but the operation of `retry` is blocking",
    note = "a blocking try cannot be canceled; retry an async operation with `retry_async` to \
            limit it by time"
)]
pub trait AcceptsTimeLimits<TimeoutTotal, TimeoutEach> {}

/// A retry of an operation, built by [`retry()`](crate::retry()) for a blocking operation, or by
/// [`retry_async()`](crate::retry_async()) for an async one.
///
/// # Settings
///
/// [`delays`](Retry::delays) is the one required setting: it says how long to wait between tries,
/// and so how many retries there are. The others are optional:
///
/// - [`when`](Retry::when) retries only the failures worth another try.
/// - [`override_next_delay`](Retry::override_next_delay) replaces the next delay, for example
///   with the `Retry-After` of an HTTP response.
/// - [`on_retry`](Retry::on_retry) runs before each retry, for example to log a warning.
/// - [`timeout_total`](Retry::timeout_total) limits how long an async retry may take in total,
///   and [`timeout_each`](Retry::timeout_each) limits each of its tries.
/// - [`sleep_with`](Retry::sleep_with) chooses how the retry waits, for example with the timer of
///   an executor other than tokio.
/// - [`config`](Retry::config) applies settings that retries share, a [`RetryConfig`].
///
/// The settings can be set in any order, with two exceptions: `config` must come first, and
/// `timeout_each` must come before `when`, `override_next_delay` and `on_retry`, because it changes
/// the failure that those closures receive. Setting something again replaces it, which is how a
/// retry changes a setting of a shared config.
///
/// The compiler rejects a retry that runs without delays, or that limits a blocking retry by
/// time:
///
/// ```compile_fail,E0277
/// use try_again::retry;
///
/// // Error: the retry has no delays.
/// let _ = retry(|| Err::<(), ()>(())).run();
/// ```
///
/// ```compile_fail,E0277
/// use try_again::{retry, DurationExt};
///
/// // Error: a blocking retry has no time limits.
/// let _ = retry(|| Err::<(), ()>(())).timeout_total(1.seconds());
/// ```
///
/// # Running
///
/// `run()` runs the retry. It tries the operation, and after each failure it waits for the next
/// delay and tries again, until a try succeeds or the retry gives up. It returns the value of the
/// first successful try, or a [`RetryError`](crate::RetryError) with the failure of the last try,
/// why the retry gave up, the number of tries and the sum of the delays.
///
/// Between tries, the retry waits with the sleep that [`sleep_with`](Retry::sleep_with) set. By
/// default, a blocking retry puts its thread to sleep (feature `std`), and an async retry sleeps
/// with tokio (feature `tokio`).
///
/// # Recipes
///
/// - [Polling until a job is done](#recipe-polling-until-a-job-is-done)
/// - [A retry budget](#recipe-a-retry-budget)
/// - [Honoring `Retry-After`](#recipe-honoring-retry-after)
/// - [Settings that a client keeps](RetryConfig)
/// - [Reconnecting, with `&mut` state](crate::retry_async_lending())
/// - [Fibonacci delays and decorrelated jitter](crate::delay#delays-of-your-own)
/// - [Testing retries on a paused clock](crate::sleep#testing-retries)
///
/// # Type parameters
///
/// `Op` is the operation, and `Config` holds its settings: it is a [`RetryConfig`]. An unset
/// setting appears as `()` in the type of the `RetryConfig`, which is what compiler errors show.
#[must_use = "a retry does nothing until `run` runs it"]
pub struct Retry<Op, Config = RetryConfig> {
    pub(super) operation: Op,
    pub(super) settings: Config,
}

impl<Op, Config: Debug> Debug for Retry<Op, Config> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Retry")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl<Op> Retry<Op> {
    /// A retry of `operation` without settings.
    pub(super) const fn new(operation: Op) -> Self {
        Self {
            operation,
            settings: RetryConfig::new(),
        }
    }
}

#[expect(
    clippy::type_complexity,
    reason = "the type of a retry records all of its settings"
)]
impl<Op, Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>
    Retry<
        Op,
        RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>,
    >
where
    Op: Operation,
    TimeoutEach: TryTimeout,
{
    /// Sets the delays to wait between the tries.
    ///
    /// The retry tries again once per delay, after waiting for it, so the delays also set how many
    /// retries there are.
    ///
    /// Take the delays from a strategy in the [`delay`](crate::delay) module, such as
    /// `Exponential::of(first).take(n)`, from an array of `Duration`s, or from any other iterator
    /// of them. They must state their limit, for example with `.take(n)` or `.forever()`; see
    /// [`ExplicitLimit`].
    ///
    /// ```
    /// # #[cfg(feature = "std")] {
    /// use try_again::delay::Exponential;
    /// use try_again::{retry, DurationExt};
    ///
    /// // Up to three retries, after 1 ms, 2 ms and 4 ms.
    /// let error = retry(|| Err::<(), _>("unavailable"))
    ///     .delays(Exponential::of(1.milliseconds()).take(3))
    ///     .run()
    ///     .unwrap_err();
    ///
    /// assert_eq!(error.tries, 4);
    /// assert_eq!(error.total_delay, 7.milliseconds());
    /// # }
    /// ```
    pub fn delays<NewDelays>(
        self,
        delays: NewDelays,
    ) -> Retry<
        Op,
        RetryConfig<
            NewDelays::IntoIter,
            TimeoutTotal,
            TimeoutEach,
            When,
            OverrideNextDelay,
            OnRetry,
            SleepWith,
        >,
    >
    where
        NewDelays: IntoIterator<Item = Duration>,
        NewDelays::IntoIter: ExplicitLimit,
    {
        self.map_settings(|settings| settings.delays(delays))
    }

    /// Gives an async retry a deadline: it gives up `timeout` after its first try started.
    ///
    /// The deadline is a hard limit: when it expires, the retry cancels the try or the delay in
    /// progress, and gives up with the reason [`Expired`](crate::StopReason::Expired). The retry
    /// also gives up early, without waiting, if the next delay could not end before the deadline.
    /// It reads no clock (see [`sleep`](crate::sleep)), so this check only adds up the delays that
    /// it has waited and the next one. It ignores the time that the tries took, so the deadline
    /// can still expire in the middle of a delay.
    ///
    /// A failure that arrives just as the deadline expires counts like any other: the retry gives
    /// up with the reason [`Rejected`](crate::StopReason::Rejected) if `when` rejects it, and with
    /// `Expired` otherwise. A deadline of zero still starts the first try, and returns its success
    /// if the try completes without waiting.
    ///
    /// A canceled try counts as a try, but leaves no failure, so the retry's error keeps the
    /// failure of the try before it, or [`TryFailure::TimedOut`](crate::TryFailure::TimedOut) if
    /// the deadline canceled the first try. The retry therefore returns
    /// `Result<T, RetryError<TryFailure<E>>>` for an operation that returns `Result<T, E>`. The
    /// closures passed to `when`, `override_next_delay` and `on_retry` still receive the
    /// operation's own failure, unless [`timeout_each`](Retry::timeout_each) wraps it.
    ///
    /// # Recipe: polling until a job is done
    ///
    /// Return `None` while the job is still running: an operation that returns an `Option` fails
    /// on `None`, so the retry checks again, here at a fixed interval until the deadline. A status
    /// check that fails ends the retry at once, because `Some(Err(error))` is not `None`:
    ///
    /// ```
    /// # #[cfg(feature = "tokio")]
    /// # #[tokio::main(flavor = "current_thread", start_paused = true)]
    /// # async fn main() {
    /// use try_again::delay::Fixed;
    /// use try_again::{retry_async, DurationExt};
    ///
    /// enum Status {
    ///     Running,
    ///     Done(String),
    /// }
    ///
    /// async fn status() -> Result<Status, std::io::Error> {
    ///     Ok(Status::Done(String::from("report.pdf")))
    /// }
    ///
    /// let polled = retry_async(|| async {
    ///     match status().await {
    ///         Ok(Status::Running) => None,
    ///         Ok(Status::Done(report)) => Some(Ok(report)),
    ///         Err(error) => Some(Err(error)),
    ///     }
    /// })
    /// .timeout_total(5.minutes())
    /// .delays(Fixed::of(10.seconds()).forever())
    /// .run()
    /// .await;
    ///
    /// // The outer `Result` tells whether the job ended in time. The inner one tells how it ended.
    /// let report: Result<String, std::io::Error> = polled.expect("the job ended in time");
    /// assert_eq!(report.unwrap(), "report.pdf");
    /// # }
    /// # #[cfg(not(feature = "tokio"))]
    /// # fn main() {}
    /// ```
    pub fn timeout_total(
        self,
        timeout: Duration,
    ) -> Retry<
        Op,
        RetryConfig<Delays, Duration, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>,
    >
    where
        Op: AsyncOperation,
    {
        self.map_settings(|settings| settings.timeout_total(timeout))
    }

    /// Cancels each try of an async retry that takes longer than `timeout`, and counts it as a
    /// failure.
    ///
    /// Each failure is then a [`TryFailure`](crate::TryFailure): a try fails with
    /// [`TryFailure::Failed`](crate::TryFailure::Failed) when the operation failed, and with
    /// [`TryFailure::TimedOut`](crate::TryFailure::TimedOut) when it timed out. The closures
    /// passed to `when`, `override_next_delay` and `on_retry` receive it, so `timeout_each` must be
    /// called before them. A try that completes just as it times out still counts.
    ///
    /// The retry's sleep measures the timeout, like the delays. Prefer this over wrapping the
    /// operation in `tokio::time::timeout`. The output of that wrapper,
    /// `Result<Result<T, E>, Elapsed>`, is a success for every try that completes, so the retry
    /// would not try again after a failure.
    ///
    /// ```
    /// # #[cfg(feature = "tokio")]
    /// # async fn example() {
    /// use try_again::delay::Exponential;
    /// use try_again::{retry_async, DurationExt, RetryError, TryFailure};
    ///
    /// async fn fetch() -> Result<String, std::io::Error> {
    ///     Ok(String::from("data"))
    /// }
    ///
    /// let out: Result<String, RetryError<TryFailure<std::io::Error>>> = retry_async(fetch)
    ///     .timeout_each(5.seconds())
    ///     .when(|failure| matches!(failure, TryFailure::TimedOut))
    ///     .delays(Exponential::of(100.milliseconds()).take(3))
    ///     .run()
    ///     .await;
    /// # }
    /// ```
    ///
    /// ```compile_fail,E0277
    /// use try_again::{retry_async, DurationExt};
    ///
    /// // Error: `timeout_each` must come before `when`, `override_next_delay` and `on_retry`.
    /// let _ = retry_async(async || Err::<(), ()>(()))
    ///     .when(|_failure| true)
    ///     .timeout_each(1.seconds());
    /// ```
    ///
    /// ```compile_fail,E0277
    /// use try_again::{retry_async, DurationExt};
    ///
    /// // Error: `timeout_each` must come before `when`, `override_next_delay` and `on_retry`.
    /// let _ = retry_async(async || Err::<(), ()>(()))
    ///     .on_retry(|_failure, _delay, _tries| {})
    ///     .timeout_each(1.seconds());
    /// ```
    ///
    /// ```compile_fail,E0277
    /// use try_again::{retry, DurationExt};
    ///
    /// // Error: a blocking retry has no time limits.
    /// let _ = retry(|| Err::<(), ()>(())).timeout_each(1.seconds());
    /// ```
    pub fn timeout_each(
        self,
        timeout: Duration,
    ) -> Retry<
        Op,
        RetryConfig<Delays, TimeoutTotal, Duration, When, OverrideNextDelay, OnRetry, SleepWith>,
    >
    where
        Op: AsyncOperation,
        (When, OverrideNextDelay, OnRetry): NoClosuresYet,
    {
        self.map_settings(|settings| settings.timeout_each(timeout))
    }

    /// Retries only the failures for which `predicate` returns `true`, and gives up on any other
    /// failure, with the reason [`Rejected`](crate::StopReason::Rejected).
    ///
    /// The predicate sees each failure before the retry takes the next delay, so a rejected
    /// failure uses up no delay. It is `FnMut`, so it may keep state, for example a count of a
    /// particular failure, or consult state that retries share.
    ///
    /// With a timeout for each try, the predicate receives a [`TryFailure`](crate::TryFailure);
    /// see [`timeout_each`](Retry::timeout_each).
    ///
    /// # Recipe: a retry budget
    ///
    /// A retry budget caps the retries of all of a client's requests together, so a service that
    /// is down does not get a multiple of its usual load. `when` checks that a token is left.
    /// `on_retry`, which runs only when a retry follows, takes the token:
    ///
    /// ```
    /// # #[cfg(feature = "tokio")]
    /// # #[tokio::main(flavor = "current_thread", start_paused = true)]
    /// # async fn main() {
    /// use std::sync::atomic::{AtomicU32, Ordering};
    /// use try_again::delay::Exponential;
    /// use try_again::{retry_async, DurationExt, StopReason};
    ///
    /// const MAX_TOKENS: u32 = 10;
    ///
    /// /// Each retry takes a token, and each success returns one.
    /// static TOKENS: AtomicU32 = AtomicU32::new(MAX_TOKENS);
    ///
    /// fn change_tokens(change: impl Fn(u32) -> Option<u32>) {
    ///     // An `Err` means that `change` left the tokens as they are.
    ///     let _ = TOKENS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, change);
    /// }
    ///
    /// async fn fetch() -> Result<String, std::io::Error> {
    ///     Err(std::io::Error::other("unavailable"))
    /// }
    ///
    /// let out = retry_async(|| async {
    ///     let out = fetch().await;
    ///     if out.is_ok() {
    ///         change_tokens(|tokens| (tokens < MAX_TOKENS).then_some(tokens + 1));
    ///     }
    ///     out
    /// })
    /// // Without a token, the retry gives up with the reason `Rejected`.
    /// .when(|_error| TOKENS.load(Ordering::Relaxed) > 0)
    /// .on_retry(|_error, _delay, _tries| change_tokens(|tokens| tokens.checked_sub(1)))
    /// .delays(Exponential::of(100.milliseconds()).take(3))
    /// .run()
    /// .await;
    ///
    /// // The three retries took three tokens. The last failure, which no retry followed, took
    /// // none.
    /// assert_eq!(out.unwrap_err().reason, StopReason::Exhausted);
    /// assert_eq!(TOKENS.load(Ordering::Relaxed), MAX_TOKENS - 3);
    /// # }
    /// # #[cfg(not(feature = "tokio"))]
    /// # fn main() {}
    /// ```
    pub fn when<Predicate>(
        self,
        predicate: Predicate,
    ) -> Retry<
        Op,
        RetryConfig<
            Delays,
            TimeoutTotal,
            TimeoutEach,
            Predicate,
            OverrideNextDelay,
            OnRetry,
            SleepWith,
        >,
    >
    where
        Predicate: FnMut(&TimeoutEach::Failure<Op::Failure>) -> bool,
    {
        self.map_settings(|settings| settings.when(predicate))
    }

    /// Replaces the delay after a failure, for example with the `Retry-After` of an HTTP
    /// response.
    ///
    /// Once `when` has accepted a failure, the retry takes its next delay and passes it, with the
    /// failure, to `override_delay`. The retry then waits for the delay that the closure returns,
    /// and `on_retry` receives that delay too. Return the delay that the closure received to keep
    /// it.
    ///
    /// The override replaces only the next delay; after the following failure, the retry goes
    /// back to its own delays. An overridden delay still uses one of them up, so a limit of
    /// `take(n)` holds, and once none are left, the retry gives up without calling the closure.
    /// The closure cannot end the retry; that is what `when` does.
    ///
    /// A [`take_total`](crate::delay::DelaysExt::take_total) budget counts each delay as
    /// scheduled, not as overridden, so it does not bound a delay that the closure returns. Cap
    /// such a delay in the closure, as below, or limit the retry with a deadline,
    /// [`timeout_total`](Retry::timeout_total), which gives up at once when an overridden delay
    /// would end after it.
    ///
    /// With a timeout for each try, the closure receives a [`TryFailure`](crate::TryFailure);
    /// see [`timeout_each`](Retry::timeout_each).
    ///
    /// # Recipe: honoring `Retry-After`
    ///
    /// [`delay::retry_after`](crate::delay::retry_after) (feature `retry-after`) reads the value
    /// of a `Retry-After` header, in seconds or as an HTTP date. Wait as long as the server asks,
    /// but at least the scheduled delay, and at most a cap of your choice:
    ///
    /// ```
    /// # #[cfg(all(feature = "tokio", feature = "retry-after"))]
    /// # #[tokio::main(flavor = "current_thread", start_paused = true)]
    /// # async fn main() {
    /// use std::time::SystemTime;
    /// use try_again::delay::{self, Exponential};
    /// use try_again::{retry_async, DurationExt};
    ///
    /// struct HttpError {
    ///     status: u16,
    ///     retry_after: Option<String>,
    /// }
    ///
    /// async fn fetch() -> Result<String, HttpError> {
    ///     Err(HttpError { status: 429, retry_after: Some(String::from("120")) })
    /// }
    ///
    /// let error = retry_async(fetch)
    ///     .when(|error| error.status == 429 || error.status == 503)
    ///     .override_next_delay(|error, delay| {
    ///         let asked = error.retry_after.as_deref().and_then(|value| {
    ///             delay::retry_after(value, SystemTime::now())
    ///         });
    ///         asked.map_or(delay, |asked| delay.max(asked.min(30.seconds())))
    ///     })
    ///     .timeout_total(1.minutes())
    ///     .delays(Exponential::of(100.milliseconds()).capped_at(10.seconds()).forever())
    ///     .run()
    ///     .await
    ///     .unwrap_err();
    ///
    /// // The server asked for 120 s, and the retry waited for the cap of 30 s. A second such wait
    /// // would have ended at the deadline, so the retry gave up without it.
    /// assert_eq!(error.tries, 2);
    /// assert_eq!(error.total_delay, 30.seconds());
    /// # }
    /// # #[cfg(not(all(feature = "tokio", feature = "retry-after")))]
    /// # fn main() {}
    /// ```
    pub fn override_next_delay<Choose>(
        self,
        override_delay: Choose,
    ) -> Retry<Op, RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, Choose, OnRetry, SleepWith>>
    where
        Choose: FnMut(&TimeoutEach::Failure<Op::Failure>, Duration) -> Duration,
    {
        self.map_settings(|settings| settings.override_next_delay(override_delay))
    }

    /// Calls `hook` right before the retry waits for the next delay, for example to log a
    /// warning.
    ///
    /// The hook receives the failure, the delay that the retry is about to wait, and the number
    /// of tries so far, the failed one included: 1 before the first retry. It runs after `when`
    /// accepted the failure and after [`override_next_delay`](Retry::override_next_delay) chose
    /// the delay. It does not run for a failure that ends the retry, including a failure after
    /// which a deadline leaves no time for another try.
    ///
    /// ```
    /// # #[cfg(feature = "std")] {
    /// use try_again::delay::Fixed;
    /// use try_again::{retry, DurationExt};
    ///
    /// let mut warnings = Vec::new();
    /// let out = retry(|| Err::<(), _>("unavailable"))
    ///     .on_retry(|error, delay, tries| {
    ///         warnings.push(format!("try {tries} failed: {error}; retrying in {delay:?}"));
    ///     })
    ///     .delays(Fixed::of(1.milliseconds()).take(2))
    ///     .run();
    ///
    /// assert_eq!(out.map_err(|error| error.failure), Err("unavailable"));
    /// assert_eq!(warnings, [
    ///     "try 1 failed: unavailable; retrying in 1ms",
    ///     "try 2 failed: unavailable; retrying in 1ms",
    /// ]);
    /// # }
    /// ```
    ///
    /// The hook is `FnMut`, so it can keep what it sees, as `warnings` does above. The `tries` of
    /// its last call are also the number of retries that a success took, which the retry itself
    /// does not return.
    ///
    /// With a timeout for each try, the hook receives a [`TryFailure`](crate::TryFailure); see
    /// [`timeout_each`](Retry::timeout_each).
    pub fn on_retry<Hook>(
        self,
        hook: Hook,
    ) -> Retry<
        Op,
        RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, Hook, SleepWith>,
    >
    where
        Hook: FnMut(&TimeoutEach::Failure<Op::Failure>, Duration, u32),
    {
        self.map_settings(|settings| settings.on_retry(hook))
    }

    /// Sets how the retry waits: for its delays and, in an async retry, to measure its time
    /// limits.
    ///
    /// Without this setting, a blocking retry puts its thread to sleep (feature `std`), and an
    /// async retry sleeps with tokio (feature `tokio`).
    ///
    /// A blocking retry takes a [`Sleep`](crate::sleep::Sleep), and an async retry takes an
    /// [`AsyncSleep`](crate::sleep::AsyncSleep): one of the sleeps in [`sleep`](crate::sleep), for
    /// another executor, the browser or embassy, or a closure:
    ///
    /// ```
    /// # #[cfg(feature = "tokio")]
    /// # async fn example() {
    /// use try_again::delay::Fixed;
    /// use try_again::{retry_async, DurationExt};
    ///
    /// let out = retry_async(async || Err::<(), ()>(()))
    ///     // A closure that returns a future, for example around the timer of another runtime.
    ///     .sleep_with(|delay| tokio::time::sleep(delay))
    ///     .delays(Fixed::of(100.milliseconds()).take(2))
    ///     .run()
    ///     .await;
    /// # }
    /// ```
    ///
    /// `sleep_with` takes sleeps that are not closures too, so the compiler cannot infer the type
    /// of a closure's parameter from it. A closure that does more with the delay than pass it on
    /// must annotate it: `|delay: Duration| ..`.
    ///
    /// ```compile_fail,E0277
    /// use std::time::Duration;
    /// use try_again::retry_async;
    ///
    /// // Error: the retry cannot sleep with this closure. It is a blocking sleep: it returns no
    /// // future.
    /// let _ = retry_async(async || Err::<(), ()>(())).sleep_with(|_delay: Duration| {});
    /// ```
    pub fn sleep_with<NewSleep>(
        self,
        sleep: NewSleep,
    ) -> Retry<
        Op,
        RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, NewSleep>,
    >
    where
        Op: AcceptsSleep<NewSleep>,
    {
        self.map_settings(|settings| settings.sleep_with(sleep))
    }

    /// Applies the settings of `config`, which retries can share; see [`RetryConfig`].
    ///
    /// The retry gets a clone of `config`: its own delays, and its own copies of the closures.
    /// `config` must come before any other setting. The settings after it add what `config` leaves
    /// unset, and replace what it set:
    ///
    /// ```
    /// # #[cfg(feature = "std")] {
    /// use try_again::delay::Fixed;
    /// use try_again::{retry, DurationExt, RetryConfig};
    ///
    /// let shared = RetryConfig::new()
    ///     .when(|status: &u16| *status >= 500)
    ///     .delays(Fixed::of(1.milliseconds()).take(5));
    ///
    /// let error = retry(|| Err::<(), u16>(503))
    ///     .config(&shared)
    ///     // This retry is less patient than the others.
    ///     .delays(Fixed::of(1.milliseconds()).take(1))
    ///     .run()
    ///     .unwrap_err();
    ///
    /// assert_eq!(error.tries, 2);
    /// # }
    /// ```
    ///
    /// ```compile_fail,E0277
    /// use try_again::{retry, RetryConfig};
    ///
    /// let shared = RetryConfig::new().delays([]);
    /// // Error: `config` must come before the other settings.
    /// let _ = retry(|| Err::<(), ()>(())).when(|()| true).config(&shared);
    /// ```
    ///
    /// ```compile_fail,E0277
    /// use try_again::{retry, DurationExt, RetryConfig};
    ///
    /// let shared = RetryConfig::new().timeout_total(1.seconds()).delays([]);
    /// // Error: a blocking retry has no time limits.
    /// let _ = retry(|| Err::<(), ()>(())).config(&shared);
    /// ```
    pub fn config<
        SharedDelays,
        SharedTimeoutTotal,
        SharedTimeoutEach,
        SharedWhen,
        SharedOverrideNextDelay,
        SharedOnRetry,
        SharedSleepWith,
    >(
        self,
        config: &RetryConfig<
            SharedDelays,
            SharedTimeoutTotal,
            SharedTimeoutEach,
            SharedWhen,
            SharedOverrideNextDelay,
            SharedOnRetry,
            SharedSleepWith,
        >,
    ) -> Retry<
        Op,
        RetryConfig<
            SharedDelays,
            SharedTimeoutTotal,
            SharedTimeoutEach,
            SharedWhen,
            SharedOverrideNextDelay,
            SharedOnRetry,
            SharedSleepWith,
        >,
    >
    where
        RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>:
            NoSettingsYet,
        RetryConfig<
            SharedDelays,
            SharedTimeoutTotal,
            SharedTimeoutEach,
            SharedWhen,
            SharedOverrideNextDelay,
            SharedOnRetry,
            SharedSleepWith,
        >: Clone,
        Op: AcceptsTimeLimits<SharedTimeoutTotal, SharedTimeoutEach>
            + AcceptsSleep<SharedSleepWith>,
        SharedTimeoutEach: TryTimeout,
        SharedWhen: WorthAnotherTry<SharedTimeoutEach::Failure<Op::Failure>>,
        SharedOverrideNextDelay: OverrideDelay<SharedTimeoutEach::Failure<Op::Failure>>,
        SharedOnRetry: RetryHook<SharedTimeoutEach::Failure<Op::Failure>>,
    {
        Retry {
            operation: self.operation,
            settings: config.clone(),
        }
    }
}

impl<Op, Config> Retry<Op, Config> {
    /// The same retry with its settings changed by `change`.
    fn map_settings<NewConfig>(
        self,
        change: impl FnOnce(Config) -> NewConfig,
    ) -> Retry<Op, NewConfig> {
        Retry {
            operation: self.operation,
            settings: change(self.settings),
        }
    }
}
