//! Races a future against a timer: a try or a delay against a timeout or a deadline.
//!
//! Both async time limits build on [`race`]: `timeout_each` races each try against its own timer,
//! and the deadline races every try and every delay against one shared timer.
//!
//! Off-the-shelf combinators do not fit:
//!
//! - The timer comes from the user's `AsyncSleep`, and `tokio` is optional, so
//!   `tokio::time::timeout` and `tokio::select!` are out, and `futures` would be a dependency for
//!   a few lines.
//! - The deadline is a single timer that keeps running across tries and delays, so [`race`]
//!   borrows it instead of consuming it. A timeout per step would need the remaining time, which
//!   means reading a clock, and the retry reads no clock: the sleep is the clock.
//! - [`race`] starts the timer before it starts the try. The timer then runs while the try does
//!   its work, including the synchronous part of the closure that creates the try's future.
//! - [`race`] reports a tie. A try that completes as the timer expires then still counts, and the
//!   caller knows not to poll the completed timer again.

use core::future::poll_fn;
use core::pin::{Pin, pin};
use core::task::Poll;

/// The outcome of a [`race`].
pub(super) struct Raced<T> {
    /// The output of the primary future, or `None` if the expiry completed first.
    pub(super) output: Option<T>,
    /// Whether the expiry completed, possibly in the same poll as the primary future. An expired
    /// expiry must not be raced again.
    pub(super) expired: bool,
}

/// Starts `primary` with `start` and runs it to completion, unless `expiry` completes first:
/// `primary` is then dropped.
///
/// Polls `expiry` before it calls `start`, because a lazy timer, such as an `async fn` that calls
/// `tokio::time::sleep`, only starts when first polled. Calling `start` first would start the
/// timer only after the closure that creates the try ran, and after the first poll of the try,
/// which may do a lot of work, or even complete it.
///
/// When both complete in the same poll, the race returns the output with `expired` set, and the
/// caller decides which wins.
///
/// `expiry` must not have completed yet, because an `async fn` future panics when polled after
/// completion. The race polls it no more once it completed, and callers stop racing it once a race
/// reports it `expired`.
pub(super) async fn race<F: Future>(
    start: impl FnOnce() -> F,
    mut expiry: Pin<&mut impl Future<Output = ()>>,
) -> Raced<F::Output> {
    let mut expired = has_expired(expiry.as_mut()).await;
    let mut primary = pin!(start());
    poll_fn(move |cx| {
        if !expired {
            expired = expiry.as_mut().poll(cx).is_ready();
        }
        match primary.as_mut().poll(cx) {
            Poll::Ready(output) => Poll::Ready(Raced {
                output: Some(output),
                expired,
            }),
            Poll::Pending if expired => Poll::Ready(Raced {
                output: None,
                expired,
            }),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

/// Polls `expiry` once, and tells whether it completed. Like [`race`], it must not be called with
/// an `expiry` that completed before.
pub(super) async fn has_expired(mut expiry: Pin<&mut impl Future<Output = ()>>) -> bool {
    poll_fn(|cx| Poll::Ready(expiry.as_mut().poll(cx).is_ready())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use core::future::{pending, ready};
    use core::task::{Context, Waker};
    use std::cell::{Cell, RefCell};

    /// Polls `future` once, with a waker that does nothing.
    fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
        pin!(future).poll(&mut Context::from_waker(Waker::noop()))
    }

    /// A timer that is pending for `polls` polls, and then completes.
    fn after_polls(polls: u32) -> impl Future<Output = ()> {
        let left = Cell::new(polls);
        poll_fn(move |cx| {
            if left.get() == 0 {
                Poll::Ready(())
            } else {
                left.set(left.get() - 1);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
    }

    #[test]
    fn the_primary_wins_against_a_pending_expiry() {
        let mut expiry = pin!(pending::<()>());

        let raced = poll_once(race(|| ready(7), expiry.as_mut()));

        let Poll::Ready(Raced { output, expired }) = raced else {
            panic!("the race must end with its primary");
        };
        assert_that!(output).is_equal_to(Some(7));
        assert_that!(expired).is_false();
    }

    #[test]
    fn the_expiry_drops_a_pending_primary() {
        // The first poll of the race polls the expiry twice: before and after it starts the
        // primary.
        /// Records in a shared flag that it was dropped.
        struct Dropped<'a>(&'a Cell<bool>);

        impl Drop for Dropped<'_> {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let dropped = Cell::new(false);
        let mut expiry = pin!(after_polls(2));
        let mut race = pin!(race(
            || {
                let guard = Dropped(&dropped);
                async move {
                    let _guard = guard;
                    pending::<()>().await;
                }
            },
            expiry.as_mut(),
        ));

        let first = race.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        let second = race.as_mut().poll(&mut Context::from_waker(Waker::noop()));

        assert_that!(first.is_pending()).is_true();
        let Poll::Ready(Raced { output, expired }) = second else {
            panic!("the race must end with its expiry");
        };
        assert_that!(output).is_equal_to(None);
        assert_that!(expired).is_true();
        assert_that!(dropped.get()).is_true();
    }

    #[test]
    fn reports_a_tie_with_the_output() {
        let mut expiry = pin!(ready(()));

        let raced = poll_once(race(|| ready("done"), expiry.as_mut()));

        let Poll::Ready(Raced { output, expired }) = raced else {
            panic!("the race must end at once");
        };
        assert_that!(output).is_equal_to(Some("done"));
        assert_that!(expired).is_true();
    }

    #[test]
    fn starts_the_expiry_before_the_primary() {
        let order = RefCell::new(Vec::new());
        let mut expiry = pin!(poll_fn(|_cx| {
            order.borrow_mut().push("expiry");
            Poll::<()>::Pending
        }));

        let _ = poll_once(race(
            || {
                order.borrow_mut().push("start");
                pending::<()>()
            },
            expiry.as_mut(),
        ));

        assert_that!(order.into_inner()).starts_with(["expiry", "start"]);
    }

    #[test]
    fn has_expired_tells_a_completed_expiry() {
        assert_that!(poll_once(has_expired(pin!(ready(()))))).is_equal_to(Poll::Ready(true));
        assert_that!(poll_once(has_expired(pin!(pending::<()>())))).is_equal_to(Poll::Ready(false));
    }
}
