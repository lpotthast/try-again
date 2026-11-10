//! [`RetryConfig`], the settings of a retry, which retries can share, and the traits that check
//! them: a retry needs delays to run, and `config` must come first.

use super::closures::NoClosuresYet;
use crate::delay::ExplicitLimit;
use core::fmt::{self, Debug, Formatter};
use core::time::Duration;

/// The settings of a retry, without its operation, so that retries can share them.
///
/// A `RetryConfig` has the settings of a [`Retry`](crate::Retry), with the same methods. Apply it
/// to a retry with [`Retry::config`](crate::Retry::config). That clones the config, so each retry
/// gets its own delays and its own copies of the closures. The settings that the retry sets after
/// that add to the config, or replace what it set.
///
/// Its type parameters record the settings, and are named after their methods: `RetryConfig<Delays,
/// TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>`. An unset setting is
/// `()`, a time limit is a `Duration`, and a sleep is its own type, such as `FuturesTimerSleep`. A
/// client on an executor other than tokio can therefore keep its sleep in the config too.
///
/// The types of closures cannot be named. A config that a struct keeps in a field therefore takes
/// its closures as function pointers, such as `fn(&E) -> bool`, and its jitter as
/// `Jitter<Take<ExponentialDelays>, fn() -> f64>`, from `.jitter(random_factor as fn() -> f64)`.
/// A function that returns a config can return its closures as `impl FnMut(&E) -> bool + Clone`.
///
/// Cloning a config clones the state of its closures too. A jitter closure that owns its random
/// number generator would therefore give every retry the same factors. Use a generator that
/// shares its state, as `delay::random_factor` does.
///
/// A config does not know the operation that it will retry, so its closures declare the type of
/// the failure that they receive. For example, an HTTP client keeps the settings of its retries:
///
/// ```
/// # #[cfg(feature = "tokio")]
/// # async fn example() {
/// use core::iter::Take;
/// use std::io;
/// use std::time::Duration;
/// use try_again::delay::{Exponential, ExponentialDelays};
/// use try_again::{retry_async, DurationExt, RetryConfig, RetryError, TryFailure};
///
/// fn is_transient(failure: &TryFailure<io::Error>) -> bool {
///     match failure {
///         TryFailure::Failed(error) => error.kind() == io::ErrorKind::ConnectionRefused,
///         TryFailure::TimedOut => true,
///     }
/// }
///
/// type Retries = RetryConfig<
///     Take<ExponentialDelays>,
///     Duration,
///     Duration,
///     fn(&TryFailure<io::Error>) -> bool,
/// >;
///
/// struct Client {
///     retries: Retries,
/// }
///
/// impl Client {
///     fn new() -> Self {
///         let retries = RetryConfig::new()
///             .timeout_total(30.seconds())
///             .timeout_each(5.seconds())
///             .when(is_transient as fn(&_) -> bool)
///             .delays(Exponential::of(100.milliseconds()).take(5));
///         Self { retries }
///     }
///
///     async fn fetch(&self, path: &str) -> Result<String, io::Error> {
///         Ok(format!("contents of {path}"))
///     }
///
///     async fn get(&self, path: &str) -> Result<String, RetryError<TryFailure<io::Error>>> {
///         retry_async(|| self.fetch(path))
///             .config(&self.retries)
///             .on_retry(|failure, delay, tries| eprintln!("try {tries}: {failure}"))
///             .run()
///             .await
///     }
/// }
/// # }
/// ```
#[derive(Clone)]
#[must_use = "a config does nothing until a retry applies it"]
pub struct RetryConfig<
    Delays = (),
    TimeoutTotal = (),
    TimeoutEach = (),
    When = (),
    OverrideNextDelay = (),
    OnRetry = (),
    SleepWith = (),
> {
    pub(super) delays: Delays,
    pub(super) timeout_total: TimeoutTotal,
    pub(super) timeout_each: TimeoutEach,
    pub(super) when: When,
    pub(super) override_next_delay: OverrideNextDelay,
    pub(super) on_retry: OnRetry,
    pub(super) sleep_with: SleepWith,
}

impl RetryConfig {
    /// Creates a config with nothing set.
    pub const fn new() -> Self {
        Self {
            delays: (),
            timeout_total: (),
            timeout_each: (),
            when: (),
            override_next_delay: (),
            on_retry: (),
            sleep_with: (),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith> Debug
    for RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>
where
    Delays: Debug,
    TimeoutTotal: Debug,
    TimeoutEach: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetryConfig")
            .field("delays", &self.delays)
            .field("timeout_total", &self.timeout_total)
            .field("timeout_each", &self.timeout_each)
            .finish_non_exhaustive()
    }
}

impl<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>
    RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>
{
    /// Sets the delays between the tries; see [`Retry::delays`](crate::Retry::delays).
    pub fn delays<NewDelays>(
        self,
        delays: NewDelays,
    ) -> RetryConfig<
        NewDelays::IntoIter,
        TimeoutTotal,
        TimeoutEach,
        When,
        OverrideNextDelay,
        OnRetry,
        SleepWith,
    >
    where
        NewDelays: IntoIterator<Item = Duration>,
        NewDelays::IntoIter: ExplicitLimit,
    {
        RetryConfig {
            delays: delays.into_iter(),
            timeout_total: self.timeout_total,
            timeout_each: self.timeout_each,
            when: self.when,
            override_next_delay: self.override_next_delay,
            on_retry: self.on_retry,
            sleep_with: self.sleep_with,
        }
    }

    /// Sets the deadline of an async retry; see
    /// [`Retry::timeout_total`](crate::Retry::timeout_total).
    pub fn timeout_total(
        self,
        timeout: Duration,
    ) -> RetryConfig<Delays, Duration, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>
    {
        RetryConfig {
            delays: self.delays,
            timeout_total: timeout,
            timeout_each: self.timeout_each,
            when: self.when,
            override_next_delay: self.override_next_delay,
            on_retry: self.on_retry,
            sleep_with: self.sleep_with,
        }
    }

    /// Sets the timeout of each try of an async retry; see
    /// [`Retry::timeout_each`](crate::Retry::timeout_each).
    pub fn timeout_each(
        self,
        timeout: Duration,
    ) -> RetryConfig<Delays, TimeoutTotal, Duration, When, OverrideNextDelay, OnRetry, SleepWith>
    where
        (When, OverrideNextDelay, OnRetry): NoClosuresYet,
    {
        RetryConfig {
            delays: self.delays,
            timeout_total: self.timeout_total,
            timeout_each: timeout,
            when: self.when,
            override_next_delay: self.override_next_delay,
            on_retry: self.on_retry,
            sleep_with: self.sleep_with,
        }
    }

    /// Sets the predicate that decides which failures are worth another try; see
    /// [`Retry::when`](crate::Retry::when).
    pub fn when<Failure, Predicate>(
        self,
        predicate: Predicate,
    ) -> RetryConfig<
        Delays,
        TimeoutTotal,
        TimeoutEach,
        Predicate,
        OverrideNextDelay,
        OnRetry,
        SleepWith,
    >
    where
        Predicate: FnMut(&Failure) -> bool,
    {
        RetryConfig {
            delays: self.delays,
            timeout_total: self.timeout_total,
            timeout_each: self.timeout_each,
            when: predicate,
            override_next_delay: self.override_next_delay,
            on_retry: self.on_retry,
            sleep_with: self.sleep_with,
        }
    }

    /// Sets the closure that overrides the next delay; see
    /// [`Retry::override_next_delay`](crate::Retry::override_next_delay).
    pub fn override_next_delay<Failure, Choose>(
        self,
        override_delay: Choose,
    ) -> RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, Choose, OnRetry, SleepWith>
    where
        Choose: FnMut(&Failure, Duration) -> Duration,
    {
        RetryConfig {
            delays: self.delays,
            timeout_total: self.timeout_total,
            timeout_each: self.timeout_each,
            when: self.when,
            override_next_delay: override_delay,
            on_retry: self.on_retry,
            sleep_with: self.sleep_with,
        }
    }

    /// Sets the hook that runs before each retry; see
    /// [`Retry::on_retry`](crate::Retry::on_retry).
    pub fn on_retry<Failure, Hook>(
        self,
        hook: Hook,
    ) -> RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, Hook, SleepWith>
    where
        Hook: FnMut(&Failure, Duration, u32),
    {
        RetryConfig {
            delays: self.delays,
            timeout_total: self.timeout_total,
            timeout_each: self.timeout_each,
            when: self.when,
            override_next_delay: self.override_next_delay,
            on_retry: hook,
            sleep_with: self.sleep_with,
        }
    }

    /// Sets the sleep that the retry waits with; see
    /// [`Retry::sleep_with`](crate::Retry::sleep_with).
    pub fn sleep_with<NewSleep>(
        self,
        sleep: NewSleep,
    ) -> RetryConfig<Delays, TimeoutTotal, TimeoutEach, When, OverrideNextDelay, OnRetry, NewSleep>
    {
        RetryConfig {
            delays: self.delays,
            timeout_total: self.timeout_total,
            timeout_each: self.timeout_each,
            when: self.when,
            override_next_delay: self.override_next_delay,
            on_retry: self.on_retry,
            sleep_with: sleep,
        }
    }
}

/// Implemented by the delays that [`delays`](crate::Retry::delays) set, and not by `()`, so a
/// retry runs only once it has delays. Sealed like [`NoSettingsYet`].
#[diagnostic::on_unimplemented(
    message = "the retry has no delays",
    label = "this retry cannot run without delays",
    note = "set them with `.delays(..)`; the delays must state their limit, with `.take(n)` or \
            `.forever()`"
)]
pub trait HasDelays: Iterator<Item = Duration> {}

impl<Delays: Iterator<Item = Duration>> HasDelays for Delays {}

/// Implemented only by a config with nothing set. [`config`](crate::Retry::config) requires it,
/// and so must be called first. The trait is public but unnameable, which keeps it sealed.
#[diagnostic::on_unimplemented(
    message = "`config` must come before the other settings",
    label = "the retry already has settings",
    note = "call `config` first: it applies all settings of a `RetryConfig` at once, and the \
            settings after it add to them or replace them"
)]
pub trait NoSettingsYet {}

impl NoSettingsYet for RetryConfig {}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn the_default_sets_nothing() {
        let debug = format!("{:?}", RetryConfig::default());

        assert_that!(debug)
            .is_equal_to("RetryConfig { delays: (), timeout_total: (), timeout_each: (), .. }");
    }
}
