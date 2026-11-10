//! The delays between the tries of a retry: the strategies [`Fixed`] and [`Exponential`], delays
//! of your own, and the limit that every retry's delays must state.
//!
//! A retry takes its delays as an iterator of [`Duration`](core::time::Duration)s, and tries
//! again once per delay.
//!
//! # Strategies
//!
//! - [`Fixed`] waits the same interval after every failure.
//! - [`Exponential`] doubles the delay after each consecutive failure, or grows it by a factor
//!   other than 2, optionally up to a cap.
//!
//! For immediate retries, use `Fixed::of(Duration::ZERO).take(n)`.
//!
//! # Limits
//!
//! A retry accepts delays only after they state their limit; see [`ExplicitLimit`]:
//!
//! - `.take(n)` allows at most `n` retries.
//! - `.take_total(budget)` takes delays while their sum stays within `budget`; see
//!   [`DelaysExt::take_total`].
//! - `.forever()` states that there is no limit.
//!
//! Arrays, `Vec`s and ranges with an end state their limit by their length, so linear delays are
//! `(1..=5).map(|n| n.seconds())`. The standard library's iterator adapters and
//! [`DelaysExt::jitter`] keep the limit, so limit the delays first, and shape them after that:
//! `Exponential::of(first).take(5).jitter(random)`.
//!
//! # Jitter
//!
//! [`DelaysExt::jitter`] multiplies each delay by a random factor, so that clients that failed
//! together do not retry together. With the feature `fastrand`, [`random_factor`] provides the
//! factors.
//!
//! # Delays of your own
//!
//! Any `Iterator<Item = Duration>` can be the delays of a retry, for example one built with
//! `iter::successors`, whose state can carry the previous delay:
//!
//! ```
//! use std::iter;
//! use std::time::Duration;
//! use try_again::delay::Fixed;
//! use try_again::DurationExt;
//!
//! // Listed delays.
//! let listed = [100.milliseconds(), 1.seconds(), 5.seconds()];
//!
//! // One immediate retry, then two after a second each.
//! let immediate_first = iter::once(Duration::ZERO).chain(Fixed::of(1.seconds()).take(2));
//!
//! // Delays that grow along the Fibonacci numbers: 1 s, 1 s, 2 s, 3 s, 5 s, ...
//! let fibonacci = iter::successors(Some((1.seconds(), 1.seconds())), |&(delay, next)| {
//!     Some((next, delay.saturating_add(next)))
//! })
//! .map(|(delay, _next)| delay)
//! .take(6);
//!
//! assert_eq!(fibonacci.last(), Some(8.seconds()));
//! ```
//!
//! "Decorrelated jitter", from the AWS article [Exponential Backoff And Jitter][aws], picks each
//! delay between the first delay and three times the previous one, up to a cap:
//!
//! ```
//! # #[cfg(feature = "fastrand")] {
//! use std::iter;
//! use try_again::delay::random_factor;
//! use try_again::DurationExt;
//!
//! let (first, cap) = (100.milliseconds(), 10.seconds());
//! let decorrelated = iter::successors(Some(first), move |&previous| {
//!     let longest = previous.saturating_mul(3);
//!     Some((first + longest.saturating_sub(first).mul_f64(random_factor())).min(cap))
//! })
//! // Draws the first delay too, so that clients do not all start with the same one.
//! .skip(1)
//! .take(5);
//! # }
//! ```
//!
//! [aws]: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
//!
//! # Delays in a loop of your own
//!
//! The strategies are also useful on their own, in a loop that a retry does not fit, for example
//! a supervisor that restarts a task:
//!
//! ```
//! use std::time::Duration;
//! use try_again::delay::Exponential;
//!
//! // The constructors are `const fn`s, so the strategy can be a constant.
//! const RESTART_DELAYS: Exponential =
//!     Exponential::of(Duration::from_millis(10)).capped_at(Duration::from_secs(300));
//!
//! fn supervise(mut task: impl FnMut() -> Result<(), String>, sleep: impl Fn(Duration)) {
//!     let mut failures = 0;
//!     while let Err(error) = task() {
//!         failures += 1;
//!         let delay = RESTART_DELAYS.delay_after(failures);
//!         eprintln!("the task failed ({error}), restarting in {delay:?}");
//!         sleep(delay);
//!     }
//! }
//!
//! let mut runs = 0;
//! supervise(
//!     || {
//!         runs += 1;
//!         if runs < 3 { Err(format!("run {runs} crashed")) } else { Ok(()) }
//!     },
//!     |_delay| {},
//! );
//! assert_eq!(runs, 3);
//! ```

mod adapters;
mod exponential;
mod fixed;
mod limit;
#[cfg(feature = "fastrand")]
mod random_factor;
#[cfg(feature = "retry-after")]
mod retry_after;

pub use adapters::{DelaysExt, Forever, Jitter, TakeTotal};
pub use exponential::{Exponential, ExponentialDelays};
pub use fixed::{Fixed, FixedDelays};
pub use limit::ExplicitLimit;
#[cfg(feature = "fastrand")]
pub use random_factor::random_factor;
#[cfg(feature = "retry-after")]
pub use retry_after::retry_after;
