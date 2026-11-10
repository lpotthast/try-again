//! [`EmbassySleep`], which sleeps with embassy's timer.

use super::AsyncSleep;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;

/// Sleeps with embassy's timer, for async retries on embedded targets without `std`.
///
/// It waits at least as long as each delay: it rounds the delay up to whole ticks of embassy's
/// clock, and adds one tick, because embassy's clock counts whole ticks and may be just short of
/// the next one when the wait starts. A delay that would end beyond embassy's latest `Instant`
/// waits until that instant.
///
/// Like any use of embassy's timer, it needs a time driver, for example that of an embassy HAL, or
/// the build fails to link. The feature `embassy-time` tracks embassy-time 0.5. With any other
/// version, pass a closure to [`sleep_with`](crate::Retry::sleep_with) instead.
///
/// ```
/// # async fn example() {
/// use try_again::delay::Fixed;
/// use try_again::sleep::EmbassySleep;
/// use try_again::{retry_async, DurationExt};
///
/// let out = retry_async(async || Err::<(), ()>(()))
///     .sleep_with(EmbassySleep)
///     .delays(Fixed::of(100.milliseconds()).take(2))
///     .run()
///     .await;
/// # }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct EmbassySleep;

impl AsyncSleep for EmbassySleep {
    type Future = EmbassyWait;

    fn sleep(&self, duration: Duration) -> EmbassyWait {
        let expires_at = embassy_time::Instant::now()
            .checked_add(to_ticks(duration))
            .unwrap_or(embassy_time::Instant::MAX);
        EmbassyWait(embassy_time::Timer::at(expires_at))
    }
}

/// One wait of [`EmbassySleep`].
///
/// It wraps embassy's `Timer`, so a new version of embassy-time does not change this crate's API.
#[derive(Debug)]
pub struct EmbassyWait(embassy_time::Timer);

impl Future for EmbassyWait {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        Pin::new(&mut self.0).poll(cx)
    }
}

/// The nanoseconds in a second.
const NANOS_PER_SEC: u128 = 1_000_000_000;

/// The ticks of embassy's clock that a wait of `duration` takes: `duration` rounded up to whole
/// ticks, plus one, at most `u64::MAX` ticks.
fn to_ticks(duration: Duration) -> embassy_time::Duration {
    let ticks = duration
        .as_nanos()
        .saturating_mul(u128::from(embassy_time::TICK_HZ))
        .div_ceil(NANOS_PER_SEC)
        .saturating_add(1);
    embassy_time::Duration::from_ticks(u64::try_from(ticks).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DurationExt;
    use assertr::prelude::*;

    fn ticks(duration: Duration) -> u64 {
        to_ticks(duration).as_ticks()
    }

    #[test]
    fn rounds_up_to_whole_ticks_and_adds_one() {
        assert_that!(ticks(Duration::ZERO)).is_equal_to(1);
        assert_that!(ticks(1.nanoseconds())).is_equal_to(2);
        assert_that!(ticks(1.seconds())).is_equal_to(embassy_time::TICK_HZ + 1);
    }

    #[test]
    fn saturates_at_the_most_ticks() {
        assert_that!(ticks(Duration::MAX)).is_equal_to(u64::MAX);
    }
}
