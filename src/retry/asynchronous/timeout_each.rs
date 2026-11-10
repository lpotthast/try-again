//! The timeout of each try, from `timeout_each`: how a try runs with and without one.

use super::AsyncOperation;
use super::race::race;
use crate::sleep::AsyncSleep;
use crate::{Fallible, TryFailure};
use core::pin::pin;
use core::time::Duration;

/// Runs one try of an async retry: `()` runs it without a timeout, and a `Duration` cancels it
/// once it takes longer. The trait is public but unnameable, which keeps it sealed.
pub trait TryTimeout {
    /// What a try fails with when its operation fails with `E`: `E` itself without a timeout, and
    /// a [`TryFailure<E>`] with one.
    type Failure<E>;

    /// Runs one try of `operation`. `sleep` measures the timeout.
    fn run_try<Op, S>(
        &self,
        operation: &mut Op,
        sleep: &S,
    ) -> impl Future<Output = Result<Op::Success, Self::Failure<Op::Failure>>>
    where
        Op: AsyncOperation,
        S: AsyncSleep;

    /// The failure of a try as a [`TryFailure`], which it already is with a timeout.
    fn into_try_failure<E>(failure: Self::Failure<E>) -> TryFailure<E>;
}

impl TryTimeout for () {
    type Failure<E> = E;

    fn run_try<Op, S>(
        &self,
        operation: &mut Op,
        _sleep: &S,
    ) -> impl Future<Output = Result<Op::Success, Op::Failure>>
    where
        Op: AsyncOperation,
        S: AsyncSleep,
    {
        // The future holds only the try's own future, and borrows neither the operation nor the
        // sleep, so that the retry's future is `Send` without a `Sync` sleep.
        let try_future = operation.call();
        async move { try_future.await.into_result() }
    }

    fn into_try_failure<E>(failure: E) -> TryFailure<E> {
        TryFailure::Failed(failure)
    }
}

impl TryTimeout for Duration {
    type Failure<E> = TryFailure<E>;

    async fn run_try<Op, S>(
        &self,
        operation: &mut Op,
        sleep: &S,
    ) -> Result<Op::Success, TryFailure<Op::Failure>>
    where
        Op: AsyncOperation,
        S: AsyncSleep,
    {
        let mut expiry = pin!(sleep.sleep(*self));
        // A try that completes as it times out still counts.
        match race(|| operation.call(), expiry.as_mut()).await.output {
            Some(out) => out.into_result().map_err(TryFailure::Failed),
            None => Err(TryFailure::TimedOut),
        }
    }

    fn into_try_failure<E>(failure: TryFailure<E>) -> TryFailure<E> {
        failure
    }
}
