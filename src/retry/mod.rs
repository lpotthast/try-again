//! The retry: [`Retry`], the builder for blocking and async operations, [`RetryConfig`], its
//! settings, and how a retry runs.
//!
//! [`retry()`] retries a blocking operation; [`retry_async()`] retries an async one. Both return
//! the same builder, and a [`RetryConfig`] holds its settings. Only an async retry has time
//! limits: it races its tries and its delays against them.

// First, so that the documentation of `Retry` lists its settings before the ways to run it.
mod builder;

mod asynchronous;
mod blocking;
mod closures;
mod config;
mod state;

pub use asynchronous::retry_async;
#[cfg(feature = "std")]
pub use asynchronous::{Loan, retry_async_lending};
pub use blocking::retry;
pub use builder::Retry;
pub use config::RetryConfig;
