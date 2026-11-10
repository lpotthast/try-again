//! The errors of a retry: [`RetryError`] for a retry that gave up, [`TryFailure`] for a try that
//! may time out, and [`NoValue`] for an operation that returned `None`.

use core::error::Error;
use core::fmt::{self, Display, Formatter};
use core::time::Duration;

/// The error of a retry that gave up: the last failure, why the retry gave up, and how many tries
/// and how much waiting it took.
///
/// A retry of an operation that returns `Result<T, E>` returns `Result<T, RetryError<E>>`. A retry
/// of an operation that returns `Option<T>` returns `Result<T, RetryError<NoValue>>`. With a
/// deadline or a timeout for each try, the failure is a [`TryFailure<E>`], because a try may then
/// end without the operation's failure.
///
/// It implements [`Error`] when the failure does, with the failure as its
/// [`source`](Error::source), so `?` passes it on to `anyhow` or a `Box<dyn Error>`. To pass on the
/// operation's own error instead, take the failure: `.run().map_err(|error| error.failure)?`.
/// You must do that when the failure does not implement `Error` itself, as is the case for an
/// `anyhow::Error` and a `Box<dyn Error>`.
///
/// Following the convention for errors with a source, it displays only why the retry gave up, for
/// example "gave up after 3 tries: no retries left". To show the failure too, print the whole
/// chain, as `anyhow` does with `{:#}`, or print the failure itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RetryError<F> {
    /// The failure of the last try that completed.
    ///
    /// A try that the deadline canceled leaves no failure, so the failure of the try before it is
    /// the last. If the deadline canceled the first try, the failure is
    /// [`TryFailure::TimedOut`].
    pub failure: F,
    /// Why the retry gave up.
    pub reason: StopReason,
    /// The number of tries that started, including the first one and a try that the deadline
    /// canceled.
    pub tries: u32,
    /// The sum of the delays that the retry waited in full, saturating at [`Duration::MAX`].
    ///
    /// This is not the time that the retry took, which also includes its tries. A delay that the
    /// deadline cut short does not count.
    pub total_delay: Duration,
}

/// Why a retry gave up.
///
/// Later versions may add reasons, so a `match` needs a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StopReason {
    /// The `when` predicate rejected the failure.
    Rejected,
    /// No retries were left: the delays ran out.
    Exhausted,
    /// The deadline of [`timeout_total`](crate::Retry::timeout_total) left no time for
    /// another try: it expired, or would have expired before the next delay ended.
    Expired,
}

impl Display for StopReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Rejected => "the failure was rejected",
            Self::Exhausted => "no retries left",
            Self::Expired => "the deadline left no time for another try",
        })
    }
}

impl<F> Display for RetryError<F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let tries = if self.tries == 1 { "try" } else { "tries" };
        write!(f, "gave up after {} {tries}: {}", self.tries, self.reason)
    }
}

impl<F: Error + 'static> Error for RetryError<F> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.failure)
    }
}

/// The failure of one try of an async retry that has a deadline or a timeout for each try: either
/// the operation failed, or the try timed out.
///
/// With [`timeout_each`](crate::Retry::timeout_each), the closures passed to `when`,
/// `override_next_delay` and `on_retry` receive this failure. With either time limit, the retry's
/// [`RetryError`] holds it.
///
/// A failed try is transparent: it displays as the operation's failure, and its
/// [`source`](Error::source) is that failure's source. Logging it, for example in `on_retry`,
/// therefore shows what went wrong. It implements [`Error`] when the operation's failure does. In
/// an error chain, code that searches for the operation's failure, for example with `anyhow`'s
/// `downcast_ref`, finds this `TryFailure` in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TryFailure<E> {
    /// The operation failed.
    Failed(E),
    /// The try took longer than its timeout, or the deadline expired during the first try, and
    /// the retry canceled it.
    TimedOut,
}

impl<E: Display> Display for TryFailure<E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(failure) => failure.fmt(f),
            Self::TimedOut => f.write_str("the try timed out"),
        }
    }
}

impl<E: Error> Error for TryFailure<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Failed(failure) => failure.source(),
            Self::TimedOut => None,
        }
    }
}

/// The failure of a try whose operation returned `None`.
///
/// An `Option` has no error of its own, so a retry of an operation that returns an `Option<T>`
/// fails with `NoValue`, and returns `Result<T, RetryError<NoValue>>`. `NoValue` implements
/// [`Error`], so `?` passes that `RetryError` on like any other. To get the `Option` back,
/// convert the result with `.run().ok()`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct NoValue;

impl Display for NoValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("the operation returned no value")
    }
}

impl Error for NoValue {}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use std::io;

    fn error<F>(failure: F, reason: StopReason, tries: u32) -> RetryError<F> {
        RetryError {
            failure,
            reason,
            tries,
            total_delay: Duration::ZERO,
        }
    }

    /// The messages of `error` and of its sources.
    fn chain(error: &(dyn Error + 'static)) -> Vec<String> {
        core::iter::successors(Some(error), |&error| error.source())
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn names_the_tries_and_the_reason() {
        let messages = [
            (StopReason::Rejected, 1),
            (StopReason::Exhausted, 3),
            (StopReason::Expired, 4),
        ]
        .map(|(reason, tries)| error((), reason, tries).to_string());

        assert_that!(messages).is_equal_to([
            "gave up after 1 try: the failure was rejected".to_owned(),
            "gave up after 3 tries: no retries left".to_owned(),
            "gave up after 4 tries: the deadline left no time for another try".to_owned(),
        ]);
    }

    #[test]
    fn keeps_the_failure_as_its_source() {
        let error = error(io::Error::other("unavailable"), StopReason::Exhausted, 3);

        assert_that!(chain(&error)).contains_exactly([
            "gave up after 3 tries: no retries left".to_owned(),
            "unavailable".to_owned(),
        ]);
    }

    #[test]
    fn a_failed_try_displays_as_the_operation_s_failure() {
        let failure = TryFailure::Failed(io::Error::other("refused"));

        assert_that!(failure.to_string()).is_equal_to("refused");
    }

    #[test]
    fn a_failed_try_shows_the_operation_s_failure_in_the_chain_once() {
        /// An error with a source of its own.
        #[derive(Debug)]
        struct Refused(io::Error);

        impl Display for Refused {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("refused")
            }
        }

        impl Error for Refused {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                Some(&self.0)
            }
        }

        let failure = TryFailure::Failed(Refused(io::Error::other("port closed")));
        let error = error(failure, StopReason::Exhausted, 2);

        assert_that!(chain(&error)).contains_exactly([
            "gave up after 2 tries: no retries left".to_owned(),
            "refused".to_owned(),
            "port closed".to_owned(),
        ]);
    }

    #[test]
    fn a_retry_of_an_option_has_an_error_chain() {
        let error = error(NoValue, StopReason::Exhausted, 2);

        assert_that!(chain(&error)).contains_exactly([
            "gave up after 2 tries: no retries left".to_owned(),
            "the operation returned no value".to_owned(),
        ]);
    }

    #[test]
    fn a_timed_out_try_has_no_source() {
        let failure: TryFailure<io::Error> = TryFailure::TimedOut;

        assert_that!(chain(&failure)).contains_exactly(["the try timed out".to_owned()]);
    }
}
