//! [`retry_async_lending`]: an async retry that lends mutable state to each of its tries.
//!
//! # Why this second entry point exists
//!
//! An async operation that needs `&mut` access to state, such as `conn.query()` with
//! `async fn query(&mut self)`, cannot be the operation of [`retry_async()`]: a closure that
//! returns a future, `FnMut() -> impl Future`, cannot lend its captures to the futures it
//! returns. An `async` closure can, so `retry_async` could accept `AsyncFnMut` operations, and
//! both `retry_async(async || conn.query().await)` and `retry_async(async move || get(&url).await)`
//! would just work.
//!
//! It does not, because stable Rust cannot show that such a retry is `Send`. Checked on Rust 1.85,
//! 1.99 and nightly 1.101 with a retry loop that is generic over `AsyncFnMut`, `tokio::spawn`
//! rejects:
//!
//! - `async || conn.query().await`, which borrows `conn` mutably, with "higher-ranked lifetime
//!   error", and
//! - `async || data.lock().await.len()` and `|| async { data.len() }`, which only borrow, with
//!   "implementation of `Send` is not general enough".
//!
//! `retry_async` spawns the last two today, so switching it to `AsyncFnMut` would break them.
//! Nightly Rust with `-Zhigher-ranked-assumptions` accepts all three.
//!
//! # When it can go
//!
//! Once stable Rust spawns all three of the retries above. Stabilizing higher-ranked assumptions
//! would do that, and so would return-type notation for the futures of async closures. Then
//! `retry_async` will take `AsyncFnMut` operations, and this entry point and [`Loan`] will be
//! deprecated in its favor.

use super::{Async, retry_async};
use crate::{Fallible, Retry};
use std::fmt::{self, Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Builds a retry of an async operation that needs mutable access to `state`, for example a
/// connection whose methods take `&mut self`.
///
/// Each try receives a [`Loan`] of the state, which dereferences to `&mut State`. The loan returns
/// the state when the try ends, even when [`timeout_each`](Retry::timeout_each) or
/// [`timeout_total`](Retry::timeout_total) cancels the try, so the next try gets the state
/// back. When the retry ends, the borrow of `state` ends too. The returned [`Retry`] is the
/// same builder that [`retry_async()`] returns.
///
/// A try can also repair the state before it uses it, for example reconnect after a failure.
/// The repair is part of the try, so the timeouts of the retry cover it too:
///
/// ```
/// # #[cfg(feature = "tokio")]
/// # async fn example() {
/// use try_again::delay::Exponential;
/// use try_again::{retry_async_lending, DurationExt};
///
/// struct Connection {
///     broken: bool,
/// }
///
/// impl Connection {
///     async fn reconnect(&mut self) -> Result<(), std::io::Error> {
///         self.broken = false;
///         Ok(())
///     }
///
///     async fn query(&mut self) -> Result<u32, std::io::Error> {
///         Ok(42)
///     }
/// }
///
/// let mut connection = Connection { broken: true };
/// let answer = retry_async_lending(&mut connection, |mut connection| async move {
///     if connection.broken {
///         connection.reconnect().await?;
///     }
///     connection.query().await
/// })
/// .timeout_each(5.seconds())
/// .delays(Exponential::of(100.milliseconds()).take(3))
/// .run()
/// .await;
///
/// // The retry returned the borrow.
/// let _ = connection.query().await;
/// # }
/// ```
///
/// It exists until [`retry_async()`] can take an `async` closure that mutates its captures, such
/// as `async || connection.query().await`, and still be spawned. This function will then be
/// deprecated.
///
/// # Panics
///
/// A try panics if it needs the state while a loan of an earlier try is still alive. A loan
/// outlives its try only if the operation leaks it, for example with `std::mem::forget`, or
/// returns it inside its failure while the retry has a deadline: the retry then keeps the last
/// failure until the next try fails. Keep the loan out of the failure.
pub fn retry_async_lending<'s, State, Op, Fut>(
    state: &'s mut State,
    mut operation: Op,
) -> Retry<Async<impl FnMut() -> Fut>>
where
    State: ?Sized,
    Op: FnMut(Loan<'s, State>) -> Fut,
    Fut: Future<Output: Fallible>,
{
    let home = Arc::new(Mutex::new(Some(state)));
    retry_async(move || operation(Loan::take(&home)))
}

/// One try's mutable access to the state of [`retry_async_lending`].
///
/// It dereferences to the state, and returns it to the retry when dropped. Take the loan as
/// `mut loan` to call methods that take `&mut self`.
pub struct Loan<'s, State: ?Sized> {
    /// The state, until the loan returns it on drop.
    state: Option<&'s mut State>,
    /// Where the state waits between tries.
    home: Arc<Mutex<Option<&'s mut State>>>,
}

impl<'s, State: ?Sized> Loan<'s, State> {
    fn take(home: &Arc<Mutex<Option<&'s mut State>>>) -> Self {
        let state = lock(home).take();
        assert!(
            state.is_some(),
            "the state of `retry_async_lending` is still lent to an earlier try: its `Loan` was \
             leaked, or is part of a failure that a retry with a deadline keeps"
        );
        Self {
            state,
            home: Arc::clone(home),
        }
    }
}

/// Locks the home of the state. No code that could panic runs while it is locked, so it is never
/// poisoned in practice.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<State: ?Sized> Deref for Loan<'_, State> {
    type Target = State;

    fn deref(&self) -> &State {
        self.state
            .as_deref()
            .expect("a loan holds the state until it is dropped")
    }
}

impl<State: ?Sized> DerefMut for Loan<'_, State> {
    fn deref_mut(&mut self) -> &mut State {
        self.state
            .as_deref_mut()
            .expect("a loan holds the state until it is dropped")
    }
}

impl<State: ?Sized> Drop for Loan<'_, State> {
    fn drop(&mut self) {
        *lock(&self.home) = self.state.take();
    }
}

impl<State: Debug + ?Sized> Debug for Loan<'_, State> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Loan").field(&&**self).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn a_loan_reads_and_shows_the_state_and_returns_it_when_dropped() {
        let mut state = 7;
        let home = Arc::new(Mutex::new(Some(&mut state)));

        let loan = Loan::take(&home);
        let read = *loan;
        let debug = format!("{loan:?}");
        drop(loan);

        assert_that!(read).is_equal_to(7);
        assert_that!(debug).is_equal_to("Loan(7)");
        assert_that!(lock(&home).is_some()).is_true();
    }
}
