// The README's examples sleep with tokio, so the README is the crate documentation only with the
// feature `tokio`. docs.rs enables it. The items' own examples build without it.
#![cfg_attr(feature = "tokio", doc = include_str!("../README.md"))]
#![cfg_attr(
    not(feature = "tokio"),
    doc = "Retry blocking and async operations with exponential backoff, jitter, per-try \
           timeouts and deadlines.\n\nStart with [`retry`] or [`retry_async`]. The [README] \
           gives an overview.\n\n[README]: https://github.com/lpotthast/try-again#readme"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// Without `std`, the crate builds for targets without an operating system. The tests use `std`.
#![cfg_attr(not(any(feature = "std", test)), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod delay;
mod duration;
mod error;
mod fallible;
mod retry;
pub mod sleep;

pub use duration::DurationExt;
pub use error::{NoValue, RetryError, StopReason, TryFailure};
pub use fallible::Fallible;
#[cfg(feature = "std")]
pub use retry::{Loan, retry_async_lending};
pub use retry::{Retry, RetryConfig, retry, retry_async};
