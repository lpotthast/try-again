//! [`TokioSleep`], which sleeps with tokio's timer.

use super::AsyncSleep;
use core::time::Duration;

/// Sleeps with tokio's timer.
///
/// The timer follows tokio's clock, so tests can pause it. It is the default sleep of async
/// retries, except in the browser (`wasm32-unknown-unknown`), where tokio has no timers. Set it
/// explicitly with [`sleep_with`](crate::Retry::sleep_with) to keep it in a
/// [`RetryConfig`](crate::RetryConfig) that blocking retries must not accept.
///
/// # Panics
///
/// [`sleep`](AsyncSleep::sleep) panics when called outside a tokio runtime, or in a runtime
/// without its time driver.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioSleep;

impl AsyncSleep for TokioSleep {
    // Unlike the other sleeps, this one exposes the timer's future: tokio's `Sleep` is part of
    // tokio's stable 1.x API, and it is `!Unpin`, so a wrapper of it would need `unsafe` code or
    // another dependency to project the pin.
    type Future = tokio::time::Sleep;

    fn sleep(&self, duration: Duration) -> tokio::time::Sleep {
        tokio::time::sleep(duration)
    }
}
