//! How a retry waits: the traits [`Sleep`] and [`AsyncSleep`], and their implementations.
//!
//! A retry waits with the sleep that [`sleep_with`](crate::Retry::sleep_with) set, or with a
//! default one. A [`RetryConfig`](crate::RetryConfig) can hold the sleep too. A retry reads no
//! clock: it measures its delays, its deadline and the timeouts of its tries by sleeping, so all
//! of them follow the sleep's clock. With tokio's sleep, that is tokio's clock, so tests that
//! pause it run without waiting in real time.
//!
//! - [`ThreadSleep`] (feature `std`) puts the thread to sleep. A blocking retry uses it by
//!   default.
//! - [`TokioSleep`] (feature `tokio`) sleeps with tokio's timer. An async retry uses it by
//!   default.
//! - [`FuturesTimerSleep`] (feature `futures-timer`) works with any executor, and in the browser.
//! - [`AsyncIoSleep`] (feature `async-io`) sleeps with the timer of smol and other executors
//!   built on async-io.
//! - [`EmbassySleep`] (feature `embassy-time`) sleeps with embassy's timer, on embedded targets
//!   without `std`.
//! - A closure is a sleep too: `|delay| monoio::time::sleep(delay)` for an async retry, or
//!   `|delay| thread::sleep(delay)` for a blocking one. A closure can also record the delays in
//!   a test.
//! - You can also implement [`Sleep`] or [`AsyncSleep`] for a type of your own. A retry takes its
//!   sleep by value, so share a sleep that has state through a closure:
//!   `|delay| shared.sleep(delay)`.
//!
//! A sleep must accept any duration, from zero up to [`Duration::MAX`], which an uncapped
//! `Exponential`, an override or a deadline can reach. All the sleeps in this module do.
//!
//! # Testing retries
//!
//! A test of a retry without time limits can record its delays instead of waiting for them; see
//! [`Sleep`]. A retry with a time limit needs a sleep that waits, because the sleep measures the
//! time limits too. On tokio's paused clock, it finishes instantly in real time, while tokio's
//! clock advances:
//!
//! ```
//! # #[cfg(feature = "tokio")]
//! #[tokio::main(flavor = "current_thread", start_paused = true)]
//! async fn main() {
//!     use try_again::delay::Fixed;
//!     use try_again::{retry_async, DurationExt, StopReason};
//!
//!     let start = tokio::time::Instant::now();
//!
//!     let error = retry_async(async || Err::<(), _>("unavailable"))
//!         .timeout_total(1.minutes())
//!         .delays(Fixed::of(10.seconds()).forever())
//!         .run()
//!         .await
//!         .unwrap_err();
//!
//!     // Tries at 0, 10, 20, 30, 40 and 50 seconds. The next delay would end at the deadline.
//!     assert_eq!(error.tries, 6);
//!     assert_eq!(error.reason, StopReason::Expired);
//!     assert_eq!(start.elapsed(), 50.seconds());
//! }
//! # #[cfg(not(feature = "tokio"))]
//! # fn main() {}
//! ```
//!
//! Time passes on a paused clock only while the tries and the delays wait. A retry whose tries
//! fail at once, with zero delays, therefore never reaches its deadline there; give it a delay
//! that is not zero, or limit it with `take(n)`.

use core::time::Duration;

/// Compiles items only where a thread can sleep: with the feature `std`, and not in the browser
/// (`wasm32-unknown-unknown`), which cannot block.
macro_rules! cfg_thread_sleep {
    ($($item:item)*) => {
        $(
            #[cfg(all(
                feature = "std",
                not(all(target_arch = "wasm32", target_os = "unknown"))
            ))]
            $item
        )*
    };
}

#[cfg(feature = "async-io")]
mod async_io_sleep;
mod default;
#[cfg(feature = "embassy-time")]
mod embassy_sleep;
#[cfg(feature = "futures-timer")]
mod futures_timer_sleep;
cfg_thread_sleep! {
    mod thread_sleep;
}
#[cfg(feature = "tokio")]
mod tokio_sleep;

#[cfg(feature = "async-io")]
pub use async_io_sleep::{AsyncIoSleep, AsyncIoWait};
#[cfg(feature = "embassy-time")]
pub use embassy_sleep::{EmbassySleep, EmbassyWait};
#[cfg(feature = "futures-timer")]
pub use futures_timer_sleep::{FuturesTimerSleep, FuturesTimerWait};
cfg_thread_sleep! {
    pub use thread_sleep::ThreadSleep;
}
#[cfg(feature = "tokio")]
pub use tokio_sleep::TokioSleep;

pub(crate) use default::{IntoAsyncSleep, IntoSleep};

/// Waits between the tries of a blocking retry.
///
/// [`ThreadSleep`] (feature `std`) is the default; [`sleep_with`](crate::Retry::sleep_with) sets
/// another. Any `Fn(Duration)` closure is a sleep too, for example one that waits on a hardware
/// timer, or that records the delays in a test. A closure that does more with the delay than pass
/// it on must annotate its parameter, `|delay: Duration| ..`:
///
/// ```
/// use std::cell::RefCell;
/// use try_again::delay::Fixed;
/// use try_again::{retry, DurationExt};
///
/// let delays = RefCell::new(Vec::new());
/// let _ = retry(|| Err::<(), ()>(()))
///     .sleep_with(|delay| delays.borrow_mut().push(delay))
///     .delays(Fixed::of(1.seconds()).take(2))
///     .run();
///
/// assert_eq!(delays.into_inner(), [1.seconds(), 1.seconds()]);
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a sleep for a blocking retry",
    note = "a blocking retry sleeps with a `Fn(Duration)` closure, or a type that implements \
            `try_again::sleep::Sleep`"
)]
pub trait Sleep {
    /// Blocks for `duration`, which may be anything from zero to [`Duration::MAX`].
    fn sleep(&self, duration: Duration);
}

// Not recommended, so that a type that is no sleep gets the message above, not a note about
// closures.
#[diagnostic::do_not_recommend]
impl<F: Fn(Duration)> Sleep for F {
    fn sleep(&self, duration: Duration) {
        self(duration);
    }
}

/// Waits between the tries of an async retry, and measures its deadline and the timeouts of its
/// tries.
///
/// [`TokioSleep`] is the default with the feature `tokio`; [`sleep_with`](crate::Retry::sleep_with)
/// sets another. Any `Fn(Duration) -> impl Future<Output = ()>` closure is a sleep too, for example
/// `|delay| monoio::time::sleep(delay)` for a runtime that this crate has no sleep for. A closure
/// that does more with the delay than pass it on must annotate its parameter, as below.
///
/// The sleep does more than wait for the delays: the retry also measures the deadline of
/// [`timeout_total`](crate::Retry::timeout_total) and the timeouts of
/// [`timeout_each`](crate::Retry::timeout_each) by sleeping. A test sleep that completes every
/// wait at once, for example to record the delays, therefore fits only a retry without time
/// limits. With a time limit, the deadline would expire and the tries would time out at once.
/// Test a retry with time limits on tokio's paused clock instead; see
/// [the module](self#testing-retries).
///
/// A sleep in the browser must not complete its waits at once, without yielding: the browser runs
/// the futures of the page before its timers, so a retry whose waits never yield would starve the
/// timer that measures its deadline. The browser's own timers, as [`FuturesTimerSleep`] uses them,
/// are fine.
///
/// The future of a sleep is an associated type, so that code that is generic over the sleep can
/// require it to be `Send`, with `S::Future: Send`. The trait itself does not require that, which
/// lets single-threaded browser timers qualify. For when a retry's future is `Send`, see
/// [`retry_async`](crate::retry_async()).
///
/// ```
/// # #[cfg(feature = "tokio")]
/// # async fn example() {
/// use std::time::Duration;
/// use try_again::delay::Fixed;
/// use try_again::{retry_async, DurationExt};
///
/// let out = retry_async(async || Err::<(), ()>(()))
///     .sleep_with(|delay: Duration| async move {
///         println!("waiting {delay:?}");
///         tokio::time::sleep(delay).await;
///     })
///     .delays(Fixed::of(100.milliseconds()).take(2))
///     .run()
///     .await;
/// # }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a sleep for an async retry",
    note = "an async retry sleeps with a `Fn(Duration) -> impl Future<Output = ()>` closure, or a \
            type that implements `try_again::sleep::AsyncSleep`"
)]
pub trait AsyncSleep {
    /// The future of one wait.
    type Future: Future<Output = ()>;

    /// Returns a future that completes after `duration`, which may be anything from zero to
    /// [`Duration::MAX`].
    fn sleep(&self, duration: Duration) -> Self::Future;
}

#[diagnostic::do_not_recommend]
impl<F, Fut> AsyncSleep for F
where
    F: Fn(Duration) -> Fut,
    Fut: Future<Output = ()>,
{
    type Future = Fut;

    fn sleep(&self, duration: Duration) -> Fut {
        self(duration)
    }
}
