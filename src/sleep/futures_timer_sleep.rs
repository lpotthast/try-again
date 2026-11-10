//! [`FuturesTimerSleep`], which sleeps with futures-timer.

use super::AsyncSleep;
use core::pin::Pin;
use core::task::{Context, Poll, ready};
use core::time::Duration;
use futures_timer::Delay;

/// Sleeps with futures-timer, which works with any executor.
///
/// futures-timer runs its timers on a thread of its own, and in the browser
/// (`wasm32-unknown-unknown`) on the browser's timers. This sleep waits whole milliseconds,
/// rounded up, so it waits at least as long as each delay. It splits a long delay into parts of at
/// most 24 days, the longest wait that the browser's `setTimeout` accepts, so any delay up to
/// [`Duration::MAX`] works.
///
/// ```
/// # async fn example() {
/// use try_again::delay::Fixed;
/// use try_again::sleep::FuturesTimerSleep;
/// use try_again::{retry_async, DurationExt};
///
/// let out = retry_async(async || Err::<(), ()>(()))
///     .sleep_with(FuturesTimerSleep)
///     .delays(Fixed::of(100.milliseconds()).take(2))
///     .run()
///     .await;
/// # }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct FuturesTimerSleep;

impl AsyncSleep for FuturesTimerSleep {
    type Future = FuturesTimerWait;

    fn sleep(&self, duration: Duration) -> FuturesTimerWait {
        let mut remaining = round_up_to_millis(duration);
        let first = take_part(&mut remaining);
        FuturesTimerWait {
            delay: Delay::new(first),
            remaining,
        }
    }
}

/// The longest wait that `setTimeout` accepts: `i32::MAX` milliseconds, about 24.8 days. Longer
/// ones overflow in futures-timer and the browser, and end at once.
const LONGEST_PART: Duration = Duration::from_millis(i32::MAX as u64);

/// One wait of [`FuturesTimerSleep`], in parts that the browser's timers accept.
#[derive(Debug)]
pub struct FuturesTimerWait {
    /// The timer of the current part.
    delay: Delay,
    /// The time left after the current part.
    remaining: Duration,
}

/// Takes the next part of a wait, at most [`LONGEST_PART`], off the time left.
fn take_part(remaining: &mut Duration) -> Duration {
    let part = (*remaining).min(LONGEST_PART);
    *remaining -= part;
    part
}

impl Future for FuturesTimerWait {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        loop {
            ready!(Pin::new(&mut self.delay).poll(cx));
            if self.remaining.is_zero() {
                return Poll::Ready(());
            }
            let part = take_part(&mut self.remaining);
            self.delay.reset(part);
        }
    }
}

/// Rounds `duration` up to whole milliseconds, the unit of the browser's timers, and at most to
/// [`Duration::MAX`].
fn round_up_to_millis(duration: Duration) -> Duration {
    const NANOS_PER_MILLI: u32 = 1_000_000;

    match duration.subsec_nanos() % NANOS_PER_MILLI {
        0 => duration,
        beyond => duration
            .checked_add(Duration::from_nanos(u64::from(NANOS_PER_MILLI - beyond)))
            .unwrap_or(Duration::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use assertr::prelude::*;

    #[test]
    fn rounds_up_to_whole_milliseconds() {
        assert_that!(round_up_to_millis(Duration::ZERO)).is_equal_to(Duration::ZERO);
        assert_that!(round_up_to_millis(1.nanoseconds())).is_equal_to(1.milliseconds());
        assert_that!(round_up_to_millis(1500.microseconds())).is_equal_to(2.milliseconds());
        assert_that!(round_up_to_millis(3.seconds())).is_equal_to(3.seconds());
        assert_that!(round_up_to_millis(Duration::MAX)).is_equal_to(Duration::MAX);
    }

    #[test]
    fn splits_a_long_wait_into_parts_that_the_browser_accepts() {
        let mut remaining = LONGEST_PART * 2 + 1.milliseconds();

        let parts = [(); 4].map(|()| take_part(&mut remaining));

        assert_that!(parts).is_equal_to([
            LONGEST_PART,
            LONGEST_PART,
            1.milliseconds(),
            Duration::ZERO,
        ]);
    }

    #[test]
    fn rounds_a_wait_up_before_splitting_it() {
        // Dropping the wait cancels its first part, the longest one.
        let wait = FuturesTimerSleep.sleep(LONGEST_PART + 1.nanoseconds());

        assert_that!(wait.remaining).is_equal_to(1.milliseconds());
    }

    #[test]
    fn waits_every_part() {
        // Arrange
        let wait = FuturesTimerWait {
            delay: Delay::new(2.milliseconds()),
            remaining: 30.milliseconds(),
        };
        let start = std::time::Instant::now();

        // Act
        block_on(wait);

        // Assert
        assert_that!(start.elapsed()).is_greater_or_equal_to(32.milliseconds());
    }

    /// Runs `future` to completion on this thread, parking it between polls.
    fn block_on(future: impl Future<Output = ()>) {
        use std::sync::Arc;
        use std::task::{Wake, Waker};
        use std::thread::{self, Thread};

        struct Unpark(Thread);

        impl Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }

        let waker = Waker::from(Arc::new(Unpark(thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut future = core::pin::pin!(future);
        while future.as_mut().poll(&mut cx).is_pending() {
            thread::park();
        }
    }
}
