//! The default sleeps: how a retry without [`sleep_with`](crate::Retry::sleep_with) waits.
//!
//! An unset setting is `()`, so `()` stands for the default sleep of each kind of retry, where
//! the target and the features provide one. The traits are public but unnameable, which keeps them
//! sealed.

use super::{AsyncSleep, Sleep};

/// The sleep of a blocking retry: the one that [`sleep_with`](crate::Retry::sleep_with) set, or
/// `()`, which puts the thread to sleep.
#[diagnostic::on_unimplemented(
    message = "the retry has no sleep",
    label = "this retry needs a sleep to wait with",
    note = "set one with `.sleep_with(..)`",
    note = "by default, a blocking retry puts its thread to sleep, but only with the feature \
            `std`, and not in the browser"
)]
pub trait IntoSleep {
    /// The sleep that the retry waits with.
    type Sleep: Sleep;

    /// Returns the sleep that the retry waits with.
    fn into_sleep(self) -> Self::Sleep;
}

cfg_thread_sleep! {
    impl IntoSleep for () {
        type Sleep = super::ThreadSleep;

        fn into_sleep(self) -> super::ThreadSleep {
            super::ThreadSleep
        }
    }
}

#[diagnostic::do_not_recommend]
impl<S: Sleep> IntoSleep for S {
    type Sleep = S;

    fn into_sleep(self) -> S {
        self
    }
}

/// The sleep of an async retry: the one that [`sleep_with`](crate::Retry::sleep_with) set, or
/// `()`, which sleeps with tokio.
#[diagnostic::on_unimplemented(
    message = "the retry has no sleep",
    label = "this retry needs a sleep to wait with",
    note = "set one with `.sleep_with(..)`, for example a sleep from `try_again::sleep`",
    note = "by default, an async retry sleeps with tokio, but only with the feature `tokio`, and \
            not in the browser"
)]
pub trait IntoAsyncSleep {
    /// The sleep that the retry waits with.
    type Sleep: AsyncSleep;

    /// Returns the sleep that the retry waits with.
    fn into_sleep(self) -> Self::Sleep;
}

// tokio has no timers in the browser (`wasm32-unknown-unknown`): a sleep would compile there, and
// panic at the first delay.
#[cfg(all(
    feature = "tokio",
    not(all(target_arch = "wasm32", target_os = "unknown"))
))]
impl IntoAsyncSleep for () {
    type Sleep = super::TokioSleep;

    fn into_sleep(self) -> super::TokioSleep {
        // Fails fast outside a tokio runtime with timers. Otherwise the retry would panic only at
        // its first delay, which a test whose first try succeeds never reaches.
        drop(tokio::time::sleep(core::time::Duration::ZERO));
        super::TokioSleep
    }
}

#[diagnostic::do_not_recommend]
impl<S: AsyncSleep> IntoAsyncSleep for S {
    type Sleep = S;

    fn into_sleep(self) -> S {
        self
    }
}
