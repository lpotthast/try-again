//! [`AsyncIoSleep`], which sleeps with async-io's timer.

use super::AsyncSleep;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;

/// Sleeps with async-io's timer, the timer of smol.
///
/// ```
/// # async fn example() {
/// use try_again::delay::Fixed;
/// use try_again::sleep::AsyncIoSleep;
/// use try_again::{retry_async, DurationExt};
///
/// let out = retry_async(async || Err::<(), ()>(()))
///     .sleep_with(AsyncIoSleep)
///     .delays(Fixed::of(100.milliseconds()).take(2))
///     .run()
///     .await;
/// # }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct AsyncIoSleep;

impl AsyncSleep for AsyncIoSleep {
    type Future = AsyncIoWait;

    fn sleep(&self, duration: Duration) -> AsyncIoWait {
        AsyncIoWait(async_io::Timer::after(duration))
    }
}

/// One wait of [`AsyncIoSleep`]: async-io's timer, without the `Instant` that it completes with.
///
/// It wraps async-io's `Timer`, so a new version of async-io does not change this crate's API.
#[derive(Debug)]
pub struct AsyncIoWait(async_io::Timer);

impl Future for AsyncIoWait {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        Pin::new(&mut self.0).poll(cx).map(|_instant| ())
    }
}
