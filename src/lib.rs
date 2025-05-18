//! # try-again
//!
//! Retry **synchronous** or **asynchronous** operations until they no longer can or need to be retried.
//!
//! Provides `fn` `retry` for retrying synchronous operations.
//!
//! Provides `async fn` `retry_async` for retrying asynchronous operations.
//!
//! Supports closures and function pointers.
//!
//! The retried operation may return any type that implements `NeedsRetry`.
//! This trait is already implemented for `Result`, `Option` and `ExitStatus`, allowing you to retry common fallible
//! outcomes.
//!
//! ## Synchronous example
//!
//! ```rust
//! use assertr::prelude::*;
//! use try_again::{delay, retry, IntoStdDuration};
//!
//! async fn do_smth() {
//!     fn fallible_operation() -> Result<(), ()> {
//!         Ok(())
//!     }
//!
//!     let final_outcome = retry(fallible_operation)
//!         .delayed_by(delay::Fixed::of(125.millis()).take(5));
//!
//!     assert_that(final_outcome).is_ok();
//! }
//! ```
//!
//! ## Asynchronous example
//!
//! ```rust
//! use assertr::prelude::*;
//! use try_again::{delay, retry_async, IntoStdDuration};
//!
//! async fn do_smth() {
//!     async fn fallible_operation() -> Result<(), ()> {
//!         Ok(())
//!     }
//!
//!     let final_outcome = retry_async(fallible_operation)
//!         .delayed_by(delay::ExponentialBackoff::of_initial_delay(125.millis()).capped_at(2.secs()).take(10))
//!         .await;
//!
//!     assert_that(final_outcome).is_ok();
//! }
//! ```
//!
//! ## Details
//!
//! ### Delay strategies
//!
//! `delayed_by` accepts a delay strategy. The `delay` module provides the following implementations
//!
//! - `None`: No delay is applied.
//! - `Fixed`: A static delay.
//! - `ExponentialBackoff`: An exponentially increasing delay
//!
//! All work with `std::time::Duration`, re-exposed as `StdDuration`. The `IntoStdDuration` can be used for a fluent syntax
//! when defining durations, like in
//!
//! use try_again::{delay, IntoStdDuration};
//!
//! delay::Fixed::of(250.millis())
//!
//! ### Delay executors
//!
//! The standard `retry` and `retry_async` functions have the following default behavior:
//!
//! - `retry` puts the current thread to sleep between retries (through the provided `ThreadSleep` executor).
//! - `retry_async` instructs the tokio runtime to sleep between retries (through the provided `TokioSleep` executor,
//!   requires the `async-tokio` feature (enabled by default)).
//!
//! The `retry_with_options` and `retry_async_with_options` functions can be used to overwrite the standard behavior
//! with any executor type implementing the `DelayExecutor` trait.
//!
//! That way, support for `async_std` or other asynchronous runtimes could be provided.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]

pub mod delay;
pub mod delay_executor;
pub mod delay_strategy;
mod duration;
mod fallible;
mod tracked_iterator;

use std::fmt::Debug;
use std::marker::PhantomData;

#[cfg(feature = "async")]
use crate::delay_executor::AsyncDelayExecutor;
use crate::delay_executor::DelayExecutor;
use crate::delay_executor::ThreadSleep;
#[cfg(feature = "async-tokio")]
use crate::delay_executor::TokioSleep;
use crate::delay_strategy::DelayStrategy;

pub use duration::IntoStdDuration;
pub use duration::StdDuration;
pub use fallible::NeedsRetry;

#[tracing::instrument(level = "debug", name = "retry", skip(operation))]
#[must_use = "Call `delayed_by` on the returned value to complete the retry strategy configuration."]
pub fn retry<Out, Op>(operation: Op) -> NeedsDelayStrategy<Out, Op>
where
    Out: NeedsRetry + Debug,
    Op: Fn() -> Out,
{
    NeedsDelayStrategy { operation }
}

pub struct NeedsDelayStrategy<Out, Op>
where
    Out: NeedsRetry + Debug,
    Op: Fn() -> Out,
{
    operation: Op,
}

impl<Out, Op> NeedsDelayStrategy<Out, Op>
where
    Out: NeedsRetry + Debug,
    Op: Fn() -> Out,
{
    pub fn delayed_by<DelayStrat>(self, delay: DelayStrat) -> Out
    where
        DelayStrat: DelayStrategy<StdDuration>,
    {
        retry_with_options(
            self.operation,
            RetryOptions {
                delay_strategy: delay,
                delay_executor: ThreadSleep,
                _marker: PhantomData,
            },
        )
    }
}

#[derive(Debug)]
pub struct RetryOptions<
    Delay: Debug + Clone,
    DelayStrat: DelayStrategy<Delay>,
    DelayExec: DelayExecutor<Delay>,
> {
    pub delay_strategy: DelayStrat,
    pub delay_executor: DelayExec,
    pub _marker: PhantomData<Delay>,
}

#[tracing::instrument(level = "debug", name = "retry_with_options", skip(operation))]
pub fn retry_with_options<Delay, DelayStrat, DelayExec, Out, Op>(
    operation: Op,
    mut options: RetryOptions<Delay, DelayStrat, DelayExec>,
) -> Out
where
    Delay: Debug + Clone,
    DelayStrat: DelayStrategy<Delay> + Debug,
    DelayExec: DelayExecutor<Delay> + Debug,
    Out: NeedsRetry + Debug,
    Op: Fn() -> Out,
{
    let mut tries: usize = 1;
    loop {
        let out = operation();
        match out.needs_retry() {
            false => return out,
            true => match options.delay_strategy.next_delay() {
                Some(delay) => {
                    tracing::debug!(tries, delay = ?delay, "Operation was not successful. Waiting...");
                    options.delay_executor.delay_by(delay.clone());
                    tries += 1;
                }
                None => {
                    tracing::error!(tries, last_output = ?out, "Operation was not successful after maximum retries. Aborting with last output seen.");
                    return out;
                }
            },
        };
    }
}

#[cfg(feature = "async")]
#[tracing::instrument(level = "debug", name = "retry_async", skip(operation))]
pub fn retry_async<Out, Op>(operation: Op) -> AsyncNeedsDelayStrategy<Out, Op>
where
    Out: NeedsRetry + Debug,
    Op: AsyncFn() -> Out,
{
    AsyncNeedsDelayStrategy { operation }
}

#[cfg(feature = "async")]
pub struct AsyncNeedsDelayStrategy<Out, Op>
where
    Out: NeedsRetry + Debug,
    Op: AsyncFn() -> Out,
{
    operation: Op,
}

#[cfg(feature = "async")]
impl<Out, Op> AsyncNeedsDelayStrategy<Out, Op>
where
    Out: NeedsRetry + Debug,
    Op: AsyncFn() -> Out,
{
    pub async fn delayed_by<DelayStrat>(self, delay: DelayStrat) -> Out
    where
        DelayStrat: DelayStrategy<StdDuration>,
    {
        retry_async_with_options(
            self.operation,
            RetryAsyncOptions {
                delay_strategy: delay,
                delay_executor: TokioSleep,
                _marker: PhantomData,
            },
        )
        .await
    }
}

#[cfg(feature = "async")]
#[derive(Debug)]
pub struct RetryAsyncOptions<
    Delay: Debug + Clone,
    DelayStrat: DelayStrategy<Delay>,
    DelayExec: AsyncDelayExecutor<Delay>,
> {
    pub delay_strategy: DelayStrat,
    pub delay_executor: DelayExec,
    pub _marker: PhantomData<Delay>,
}

#[cfg(feature = "async")]
#[tracing::instrument(
    level = "debug",
    name = "retry_async_with_delay_strategy",
    skip(operation)
)]
pub async fn retry_async_with_options<Delay, DelayStrat, DelayExec, Out>(
    operation: impl AsyncFn() -> Out,
    mut options: RetryAsyncOptions<Delay, DelayStrat, DelayExec>,
) -> Out
where
    Delay: Debug + Clone,
    DelayStrat: DelayStrategy<Delay>,
    DelayExec: AsyncDelayExecutor<Delay>,
    Out: NeedsRetry + Debug,
{
    let mut tries: usize = 1;
    loop {
        let out = operation().await;
        match out.needs_retry() {
            false => return out,
            true => match options.delay_strategy.next_delay() {
                Some(delay) => {
                    tracing::debug!(tries, delay = ?delay, "Operation was not successful. Waiting...");
                    options.delay_executor.delay_by(delay.clone()).await;
                    tries += 1;
                }
                None => {
                    tracing::error!(tries, last_output = ?out, "Operation was not successful after maximum retries. Aborting with last output seen.");
                    return out;
                }
            },
        };
    }
}
