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

/// Exponentially back off until the operation returns `Result::Ok` or the maximum number of retries is reached.
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

/// Exponentially back off until the operation returns `Result::Ok` or the maximum number of retries is reached.
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
    DelayFut: Future<Output = ()> + Send,
    DelayStrat: DelayStrategy<D, Out = DelayFut> + Debug,
    Out: NeedsRetry + Debug,
    Fut: Future<Output = Out> + Send,
    Fut::Output: Send,
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
