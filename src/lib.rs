//! # try-again
//!
//! Retry **synchronous** or **asynchronous** operations until they no longer can or need to be retried.
//!
//! Provides `fn retry` for retrying synchronous operations.
//!
//! Provides `async fn retry_async` for retrying asynchronous operations.
//!
//! The retried closure may return any type that implements `NeedsRetry`. This trait is already implemented for any `Result` and `Option`, allowing you to retry common fallible outcomes.
//!
//! A retry strategy is required. The provided `Retry` type provides an implementation supporting
//!
//! - A maximum number of retries
//! - A delay between retries being either:
//!   - None
//!   - A static delay
//!   - An exponentially increasing delay
//!
//! A delay strategy is required and performs the actual delaying between executions of the users closure:
//!
//! - In the synchronous case: `ThreadSleep {}` can be used, blocking the current thread until the next try should take place.
//! - In the asynchronous case: `TokioSleep {}` can be used when using the Tokio runtime.
//!
//! Other delay strategies can be implemented to for example support async_std or other asynchronous runtimes.
//!
//! ## Synchronous example
//!
//! ```Rust
//! use try_again::{retry, Delay, Retry, ThreadSleep};
//!
//! fn some_fallible_operation() -> Result<(), ()> {
//!     Ok(())
//! }
//!
//! let final_outcome = retry(
//!     Retry {
//!         max_tries: 5,
//!         delay: Some(Delay::Static {
//!             delay: Duration::from_millis(125),
//!         }),
//!     },
//!     ThreadSleep {},
//!     move || some_fallible_operation(),
//! ).await;
//! ```
//!
//! ## Asynchronous example
//!
//! ```Rust
//! use try_again::{retry_async, Delay, Retry, TokioSleep};
//!
//! async fn some_fallible_operation() -> Result<(), ()> {
//!     Ok(())
//! }
//!
//! let final_outcome = retry_async(
//!     Retry {
//!         max_tries: 10,
//!         delay: Some(Delay::ExponentialBackoff {
//!             initial_delay: Duration::from_millis(125),
//!             max_delay: Some(Duration::from_secs(2)),
//!         }),
//!     },
//!     TokioSleep {},
//!     move || async move {
//!         some_fallible_operation().await
//!     },
//! ).await;
//! ```

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]

use std::fmt::Debug;
use std::future::Future;
use tracing::debug;
use tracing::error;

mod delay;
mod retry;

pub use delay::DelayStrategy;
pub use delay::ThreadSleep;
#[cfg(feature = "async-tokio")]
pub use delay::TokioSleep;
pub use retry::Delay;
pub use retry::NeedsRetry;
pub use retry::Retry;
pub use retry::RetryStrategy;

/// Retries the given `operation` until its outcome needs no more retries or the `retry_strategy` states that it is exhausted.
/// Uses the given `delay_strategy` to preform the "waiting" between retries.
/// You may use `try_again::ThreadSleep {}` as the delay strategy, blocking the current thread between retries.
#[tracing::instrument(level = "debug", name = "retry", skip(operation))]
pub fn retry<D, RetryStrat, DelayStrat, Out, Op>(
    retry_strategy: RetryStrat,
    delay_strategy: DelayStrat,
    operation: Op,
) -> Out
where
    D: Debug + Clone,
    RetryStrat: RetryStrategy<D> + Debug,
    DelayStrat: DelayStrategy<D> + Debug,
    Out: NeedsRetry + Debug,
    Op: Fn() -> Out,
{
    let mut tries: usize = 1;
    let mut next_delay = retry_strategy.initial_delay();
    loop {
        let out = operation();
        match out.needs_retry() {
            false => return out,
            true => {
                if retry_strategy.is_exhausted(tries) {
                    error!(tries, last_output = ?out, "Operation was not successful after maximum retries. Aborting with last output seen.");
                    return out;
                } else {
                    debug!(tries, delay = ?next_delay, "Operation was not successful. Waiting...");
                    delay_strategy.delay(next_delay.clone());
                    next_delay = retry_strategy.next_delay(tries, next_delay);
                    tries += 1;
                }
            }
        };
    }
}

/// Retries the given asynchronous `operation` until its outcome needs no more retries or the `retry_strategy` states that it is exhausted.
/// Uses the given `delay_strategy` to preform the "waiting" between retries.
/// You may use `try_again::TokioSleep {}` as the delay strategy when using the Tokio runtime.
#[cfg(feature = "async")]
#[tracing::instrument(level = "debug", name = "retry_async", skip(operation))]
pub async fn retry_async<D, RetryStrat, DelayFut, DelayStrat, Out, Fut, Op>(
    retry_strategy: RetryStrat,
    delay_strategy: DelayStrat,
    operation: Op,
) -> Out
where
    D: Debug + Clone,
    RetryStrat: RetryStrategy<D> + Debug,
    DelayFut: Future<Output = ()>,
    DelayStrat: DelayStrategy<D, Out = DelayFut> + Debug,
    Out: NeedsRetry + Debug,
    Fut: Future<Output = Out>,
    Op: Fn() -> Fut,
{
    let mut tries: usize = 1;
    let mut next_delay = retry_strategy.initial_delay();
    loop {
        let out = operation().await;
        match out.needs_retry() {
            false => return out,
            true => {
                if retry_strategy.is_exhausted(tries) {
                    error!(tries, last_output = ?out, "Operation was not successful after maximum retries. Aborting with last output seen.");
                    return out;
                } else {
                    debug!(tries, delay = ?next_delay, "Operation was not successful. Waiting...");
                    delay_strategy.delay(next_delay.clone()).await;
                    next_delay = retry_strategy.next_delay(tries, next_delay);
                    tries += 1;
                }
            }
        };
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::AtomicI32;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::retry;
    use crate::retry_async;
    use crate::Delay;
    use crate::Retry;
    use crate::ThreadSleep;

    #[test]
    fn retry_on_success_never_retries() {
        fn successful(counter: Arc<AtomicI32>) -> Result<i32, ()> {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            let counter = counter.clone();
            retry(Retry::max_tries(3), ThreadSleep {}, move || {
                successful(counter.clone())
            })
        };

        assert_eq!(Ok(42), out);
        assert_eq!(
            1,
            counter.load(Ordering::SeqCst),
            "Function must have been called 1 time only!"
        );
    }

    #[test]
    fn retry_on_continuous_error_retries_expected_number_of_times() {
        fn erroneous(counter: Arc<AtomicI32>) -> Result<(), i32> {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            let counter = counter.clone();
            retry(
                Retry {
                    max_tries: 3,
                    delay: Some(Delay::Static {
                        delay: Duration::from_millis(50),
                    }),
                },
                ThreadSleep {},
                move || erroneous(counter.clone()),
            )
        };

        assert_eq!(Err(42), out);
        assert_eq!(
            3,
            counter.load(Ordering::SeqCst),
            "Function must have been called 3 times!"
        );
    }

    #[tokio::test]
    async fn retry_async_on_continuous_error_retries_expected_number_of_times() {
        use crate::TokioSleep;

        async fn erroneous(counter: Arc<AtomicI32>) -> Result<(), i32> {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            let counter = counter.clone();
            retry_async(
                Retry {
                    max_tries: 3,
                    delay: Some(Delay::Static {
                        delay: Duration::from_millis(50),
                    }),
                },
                TokioSleep {},
                move || {
                    let counter = counter.clone();
                    async move { erroneous(counter).await }
                },
            )
            .await
        };

        assert_eq!(Err(42), out);
        assert_eq!(
            3,
            counter.load(Ordering::SeqCst),
            "Function must have been called 3 times!"
        );
    }
}
