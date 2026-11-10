//! Tests of async retries.

use assertr::prelude::*;
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::time::Instant;
use try_again::StopReason::{Exhausted, Expired, Rejected};
use try_again::delay::{Exponential, Fixed};
use try_again::sleep::AsyncSleep;
use try_again::{DurationExt, NoValue, RetryConfig, RetryError, TryFailure, retry_async};

#[path = "../common/mod.rs"]
mod common;
use common::{Calls, FailureAliveDuringDelay, Held, RecordedDelays};

// Sleeps with tokio's clock, which the tests pause, even without the feature `tokio`. A function
// is a sleep like a closure. The module `sleeps` covers `run()` and `TokioSleep`, which need the
// feature.
use tokio::time::sleep as tokio_clock;

/// Completes a zero-length wait at once, without yielding, as some timers do, and sleeps with
/// tokio otherwise.
async fn eager_zero(duration: Duration) {
    if !duration.is_zero() {
        tokio::time::sleep(duration).await;
    }
}

/// Connects on the third try, counted in `tries`, which a `Send` future can share.
#[expect(clippy::unused_async, reason = "stands in for an async operation")]
async fn connect(tries: &AtomicU32) -> Result<u32, std::io::Error> {
    let try_number = tries.fetch_add(1, Ordering::SeqCst) + 1;
    if try_number < 3 {
        Err(std::io::Error::other("refused"))
    } else {
        Ok(try_number)
    }
}

mod config;
mod delays;
#[cfg(feature = "std")]
mod lending;
mod on_retry;
mod operations;
mod override_next_delay;
mod resources;
mod send;
#[cfg(any(
    feature = "tokio",
    feature = "futures-timer",
    feature = "async-io",
    feature = "embassy-time"
))]
mod sleeps;
mod timeout_each;
mod timeout_total;
mod when;
