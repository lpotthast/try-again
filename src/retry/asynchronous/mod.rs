//! The async retry: [`retry_async`], and how it runs without a deadline. Its time limits have
//! modules of their own.

#[cfg(feature = "std")]
mod lending;
mod race;
mod timeout_each;
mod timeout_total;

#[cfg(feature = "std")]
pub use lending::{Loan, retry_async_lending};
pub use timeout_each::TryTimeout;

use super::RetryConfig;
use super::builder::{AcceptsSleep, AcceptsTimeLimits, Operation, Retry};
use super::closures::{OverrideDelay, RetryHook, WorthAnotherTry};
use super::config::HasDelays;
use super::state::{Next, RetryState};
use crate::sleep::{AsyncSleep, IntoAsyncSleep};
use crate::{Fallible, RetryError};
use core::future::poll_fn;
use core::task::Poll;

/// Builds a retry of an async operation, a closure that returns a future of a [`Fallible`] such
/// as a `Result` or an `Option`.
///
/// Set its delays with [`delays`](Retry::delays), and run it with `run().await`; see [`Retry`]
/// for all settings, including the time limits that only an async retry has.
///
/// The operation is an `async fn`, an `async` closure, or a closure that returns a future, such as
/// `|| async { .. }`. It can mutate what it captures before it creates the future, for example to
/// count the tries:
///
/// ```
/// # #[cfg(feature = "tokio")]
/// # async fn example() {
/// use try_again::delay::Fixed;
/// use try_again::{retry_async, DurationExt, RetryError};
///
/// async fn fetch(try_number: u32) -> Result<String, std::io::Error> {
///     Ok(format!("data from try {try_number}"))
/// }
///
/// let mut tries = 0;
/// let out: Result<String, RetryError<std::io::Error>> = retry_async(|| {
///     tries += 1;
///     fetch(tries)
/// })
/// .delays(Fixed::of(100.milliseconds()).take(3))
/// .run()
/// .await;
/// # }
/// ```
///
/// # Borrowing from the operation
///
/// The futures of the operation cannot borrow from the operation itself, because stable Rust
/// cannot yet show that a retry of such an operation is `Send`. So an `async move` closure whose
/// future borrows what the closure owns does not compile:
///
/// ```compile_fail
/// use try_again::retry_async;
///
/// async fn get(url: &str) -> Result<String, std::io::Error> {
///     Ok(url.to_owned())
/// }
///
/// let url = String::from("https://example.com");
/// // Error: async closure does not implement `FnMut` because it captures state from its
/// // environment.
/// let _ = retry_async(async move || get(&url).await);
/// ```
///
/// Borrow the value outside the closure instead, and the closure can borrow it from there:
///
/// ```
/// # use try_again::retry_async;
/// # async fn get(url: &str) -> Result<String, std::io::Error> {
/// #     Ok(url.to_owned())
/// # }
/// let url = String::from("https://example.com");
/// let url = &url;
/// let _ = retry_async(async || get(url).await);
/// ```
///
/// For an operation that needs `&mut` access to its state, for example `connection.query()` with
/// `query(&mut self)`, use [`retry_async_lending`](crate::retry_async_lending()) (feature `std`)
/// instead. It lends the state to each try.
///
/// Without `std`, or in a task that need not be `Send`, a `RefCell` lends the state. The tries
/// of a retry never overlap, and a try that a time limit cancels drops its borrow, so the borrow
/// never fails:
///
/// ```
/// # use try_again::retry_async;
/// # struct Uart;
/// # impl Uart {
/// #     async fn read(&mut self) -> Result<u8, ()> {
/// #         Ok(1)
/// #     }
/// # }
/// use core::cell::RefCell;
///
/// let mut uart = Uart;
/// let uart = RefCell::new(&mut uart);
/// let _ = retry_async(|| async { uart.borrow_mut().read().await });
/// ```
///
/// # Spawning
///
/// `tokio::spawn` accepts the retry's future only if it is `Send`. It is `Send` when everything
/// that it holds across a wait is: the operation and its futures, the delays, the closures, the
/// sleep, and the sleep's futures. With a time limit, the retry also borrows the sleep across
/// waits, so the sleep must be `Sync`, as `TokioSleep` is. With a deadline, the retry keeps the
/// last failure across the delay and the next try, so the failure must be `Send` too.
pub fn retry_async<Op, Fut>(operation: Op) -> Retry<Async<Op>>
where
    Op: FnMut() -> Fut,
    Fut: Future<Output: Fallible>,
{
    Retry::new(Async(operation))
}

/// The operation of an async retry, from [`retry_async`]: a closure that returns a future. The
/// type is public but unnameable.
pub struct Async<Op>(Op);

impl<Op, Fut> Operation for Async<Op>
where
    Op: FnMut() -> Fut,
    Fut: Future<Output: Fallible>,
{
    type Success = <Fut::Output as Fallible>::Success;
    type Failure = <Fut::Output as Fallible>::Failure;
}

/// Starts the tries of an async retry. Only an async retry has time limits, so the time limits
/// require it. The trait is public but unnameable, which keeps it sealed.
#[diagnostic::on_unimplemented(
    message = "a blocking retry has no time limits",
    label = "the operation of `retry` is blocking",
    note = "a blocking try cannot be canceled; retry an async operation with `retry_async` to \
            limit it by time"
)]
pub trait AsyncOperation: Operation {
    /// The future of one try.
    //
    // A named type, rather than an `impl Future` of `call`, keeps the retry's future `Send` for
    // operations that borrow: the futures that the retry holds across waits then name no lifetime
    // that the compiler would have to prove `Send` for all lifetimes.
    type Future: Future<Output: Fallible<Success = Self::Success, Failure = Self::Failure>>;

    /// Starts one try.
    fn call(&mut self) -> Self::Future;
}

impl<Op, Fut> AsyncOperation for Async<Op>
where
    Op: FnMut() -> Fut,
    Fut: Future<Output: Fallible>,
{
    type Future = Fut;

    fn call(&mut self) -> Fut {
        (self.0)()
    }
}

impl<Op, TimeoutTotal, TimeoutEach> AcceptsTimeLimits<TimeoutTotal, TimeoutEach> for Async<Op> {}

impl<Op> AcceptsSleep<()> for Async<Op> {}

#[diagnostic::do_not_recommend]
impl<Op, S: AsyncSleep> AcceptsSleep<S> for Async<Op> {}

impl<Op, Delays, TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>
    Retry<Op, RetryConfig<Delays, (), TimeoutEach, When, OverrideNextDelay, OnRetry, SleepWith>>
where
    Op: AsyncOperation,
    TimeoutEach: TryTimeout,
    When: WorthAnotherTry<TimeoutEach::Failure<Op::Failure>>,
    OverrideNextDelay: OverrideDelay<TimeoutEach::Failure<Op::Failure>>,
    OnRetry: RetryHook<TimeoutEach::Failure<Op::Failure>>,
{
    /// Runs an async retry.
    ///
    /// Between tries it waits with the sleep that [`sleep_with`](Retry::sleep_with) set, or with
    /// tokio if none was set.
    ///
    /// # Errors
    ///
    /// Returns a [`RetryError`] with the last failure if the retry gave up. With a timeout for
    /// each try, that failure is a [`TryFailure`](crate::TryFailure).
    ///
    /// # Panics
    ///
    /// With tokio's sleep, the default, it panics when it is first polled outside a tokio runtime,
    /// or in a runtime without the time driver; see [`TokioSleep`](crate::sleep::TokioSleep).
    pub async fn run(self) -> Result<Op::Success, RetryError<TimeoutEach::Failure<Op::Failure>>>
    where
        Delays: HasDelays,
        SleepWith: IntoAsyncSleep,
    {
        let Self {
            mut operation,
            settings,
        } = self;
        let sleep = settings.sleep_with.into_sleep();
        let mut state = RetryState::new(
            settings.delays,
            settings.when,
            settings.override_next_delay,
            settings.on_retry,
            None,
        );
        loop {
            // The scope drops a failure before the delay. The retry's future is then `Send` even
            // when the failure is not, and the failure holds no resources while the retry waits.
            let delay = {
                let failure = match settings.timeout_each.run_try(&mut operation, &sleep).await {
                    Ok(success) => return Ok(success),
                    Err(failure) => failure,
                };
                match state.after(&failure) {
                    Next::Retry(delay) => delay,
                    Next::Stop(reason) => return Err(state.gave_up(failure, reason)),
                }
            };
            sleep.sleep(delay).await;
            state.waited(delay);
            yield_now().await;
        }
    }
}

/// Returns `Pending` once, after it wakes its task, to let the executor run other tasks and
/// timers.
///
/// An async retry yields once per retry: with a sleep that completes zero-length waits at once,
/// and an operation that fails at once, its future would otherwise never return `Pending`, and
/// neither its deadline nor an outer timeout could stop it.
///
/// This helps only where an executor still drives its timers while a task keeps waking itself,
/// as tokio does every few dozen polls. In the browser, wasm-bindgen-futures runs a woken task
/// as a microtask, before any timer, so yielding there does not let the deadline's timer fire.
/// That is why a sleep in the browser must not complete its waits at once; see
/// [`AsyncSleep`](crate::sleep::AsyncSleep).
async fn yield_now() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use core::pin::pin;
    use core::task::{Context, Waker};

    #[test]
    fn yield_now_is_pending_once() {
        let mut yielded = pin!(yield_now());
        let mut cx = Context::from_waker(Waker::noop());

        assert_that!(yielded.as_mut().poll(&mut cx).is_pending()).is_true();
        assert_that!(yielded.as_mut().poll(&mut cx).is_ready()).is_true();
    }
}
