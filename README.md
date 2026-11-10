# try-again

[![Crates.io][crates-badge]][crates]
[![Docs.rs][docs-badge]][docs]
[![CI][ci-badge]][ci]
[![Coverage][coverage-badge]][coverage]
[![MSRV][msrv-badge]][cargo-toml]
[![License: MIT OR Apache-2.0][license-badge]](#license)

Retry blocking and async operations with exponential backoff, jitter, per-try timeouts and deadlines.

```toml
[dependencies]
try-again = "0.3"
```

A blocking retry:

```rust
use try_again::{delay, retry, DurationExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // One try, then at most 5 retries, after 100 ms, 200 ms, 400 ms, 800 ms and 1.6 s.
    let manifest = retry(|| std::fs::read_to_string("Cargo.toml"))
        .delays(delay::Exponential::of(100.milliseconds()).take(5))
        .run()?;
    println!("{manifest}");
    Ok(())
}
```

`100.milliseconds()` comes from the trait [`DurationExt`][DurationExt]. Any `std::time::Duration` works in its place.

An async retry with time limits:

```rust
use std::io::ErrorKind;
use try_again::{delay, retry_async, DurationExt, RetryError, TryFailure};

async fn connect() -> Result<(), std::io::Error> {
    Ok(())
}

// Retry refused connections with growing delays, give each try 5 seconds, and give up after 30.
async fn connect_with_retries() -> Result<(), RetryError<TryFailure<std::io::Error>>> {
    retry_async(connect)
        .timeout_each(5.seconds())
        .when(|failure| match failure {
            TryFailure::Failed(error) => error.kind() == ErrorKind::ConnectionRefused,
            TryFailure::TimedOut => true,
        })
        .on_retry(|failure, delay, tries| eprintln!("try {tries} failed: {failure}; retrying in {delay:?}"))
        .timeout_total(30.seconds())
        .delays(delay::Exponential::of(100.milliseconds()).capped_at(5.seconds()).forever())
        .run()
        .await
}
```

A retry that gives up returns an error that says what happened:

```rust
use try_again::{retry, DurationExt, StopReason};

let error = retry(|| Err::<(), _>("connection refused"))
    .delays([10.milliseconds(), 20.milliseconds()])
    .run()
    .unwrap_err();

assert_eq!(error.to_string(), "gave up after 3 tries: no retries left");
assert_eq!(error.failure, "connection refused");
assert_eq!(error.reason, StopReason::Exhausted);
assert_eq!(error.total_delay, 30.milliseconds());
```

## Why try-again

- **Every retry states its limit.** Delays must come from a finite source, such as an array, or state their limit with
  `.take(n)`, `.take_total(budget)` or `.forever()`. Anything else does not compile. No limit hides in a default.
- **Hard time limits.** `timeout_total` gives an async retry a deadline: when it expires, the retry cancels the try or
  the delay in progress. `timeout_each` cancels a try that takes too long. Neither waits for a try to finish first.
- **No clock reads.** The retry's sleep measures every delay, deadline and timeout. Tests on tokio's paused clock
  therefore finish at once, and the same retry runs on embassy with `sleep_with(EmbassySleep)`, and in the browser
  with `sleep_with(FuturesTimerSleep)`. Where tokio's sleep is not available, the compiler asks for one.
- **Server-chosen delays.** `override_next_delay` lets a failure set the next delay, for example from a `Retry-After`
  header. `delay::retry_after` reads that header, in seconds or as an HTTP date.
- **Errors that say what happened.** A `RetryError` holds the last failure, why the retry gave up, the number of tries
  and the total delay. It keeps the failure in its error chain.
- **Mistakes that do not compile.** A retry without delays, a time limit on a blocking retry, or a sleep of the wrong
  kind is a compiler error, with a message that says so.
- **Blocking, async, `no_std` and WebAssembly**, with sleeps for tokio, smol, embassy, the browser, or any closure.

## Retrying

[`retry(operation)`][retry] retries a blocking operation; [`retry_async(operation)`][retry_async] retries an async
one: an `async fn`, an `async` closure or `|| async { .. }`. The operation returns a `Result`, an `Option`, or
anything else that implements [`Fallible`][Fallible].

`delays(..)` is the one required setting: it says how long to wait between tries, and so how many retries there are.
`run()` runs the retry: it tries the operation, and after each failure waits for the next delay and tries again, until
a try succeeds or the delays run out.

The other settings are optional:

- `when(|failure| ..)` retries only the failures worth another try, and gives up on any other.
- `on_retry(|failure, delay, tries| ..)` runs before each retry, for example to log a warning.
- `override_next_delay(|failure, delay| ..)` replaces the next delay, for example with an HTTP `Retry-After`.
- `timeout_total(timeout)` gives an async retry a deadline.
- `timeout_each(timeout)` gives each try of an async retry a timeout.
- `sleep_with(sleep)` sets how the retry waits; see [Sleeping](#sleeping).
- `config(&shared)` applies settings that retries share; see [Sharing settings](#sharing-settings).

The settings can be set in any order, with two exceptions. `config` must be called first. `timeout_each` must be
called before `when`, `override_next_delay` and `on_retry`, because it changes the failure that those closures receive
to a [`TryFailure`][TryFailure]. Setting something again replaces it.

An `async move` closure whose future borrows what the closure owns, such as `async move || get(&url).await`, does not
compile yet. Borrow the value outside the closure instead: `let url = &url;`, then `async || get(url).await`. For an
async operation that needs `&mut` state, such as a connection whose methods take `&mut self`, use
[`retry_async_lending(&mut state, |state| ..)`][retry_async_lending], which lends the state to each try.

## Errors

A retry returns the value of the first successful try, or a [`RetryError`][RetryError] with the failure of the last
try, why the retry gave up, the number of tries, and the sum of the delays that it waited. The failure is the
operation's own, with two exceptions:

- An operation that returns an `Option` fails with [`NoValue`][NoValue]. `.run().ok()` gives the `Option` back.
- With a time limit, the failure is a [`TryFailure<E>`][TryFailure]: either the operation's failure, or a try that
  timed out.

`RetryError` implements `Error` when the failure does, with the failure as its source, so `?` passes it on to `anyhow`
or a `Box<dyn Error>`. It displays why the retry gave up, as in "gave up after 3 tries: no retries left", and leaves
the failure to the error chain.

If the operation's error is itself an `anyhow::Error` or a `Box<dyn Error>`, which do not implement `Error`, `?` does
not compile. Pass that error on instead, here with the number of tries as its context:

```rust,ignore
// `fetch` returns an `anyhow::Result<String>`.
let page = retry(fetch)
    .delays(Exponential::of(100.milliseconds()).take(3))
    .run()
    .map_err(|error| error.failure.context(format!("gave up after {} tries", error.tries)))?;
```

A `Box<dyn Error>` has no `context`: `.run().map_err(|error| error.failure)?` passes it on as it is.

## Sharing settings

A [`RetryConfig`][RetryConfig] holds the settings of a retry without its operation. A client can keep one, for example
in a field, and apply it to each of its retries with `config(&shared)`:

```rust
use std::io;
use try_again::delay::{Exponential, ExponentialDelays};
use try_again::{retry_async, DurationExt, RetryConfig, RetryError};

struct Client {
    retries: RetryConfig<std::iter::Take<ExponentialDelays>>,
}

impl Client {
    async fn fetch(&self, path: &str) -> Result<String, io::Error> {
        Ok(format!("contents of {path}"))
    }

    async fn get(&self, path: &str) -> Result<String, RetryError<io::Error>> {
        retry_async(|| self.fetch(path)).config(&self.retries).run().await
    }
}

let client = Client { retries: RetryConfig::new().delays(Exponential::of(100.milliseconds()).take(3)) };
```

`config` must be called before any other setting. The settings after it add to the config, or replace what it set:
`.config(&self.retries).delays(..)` gives one retry delays of its own.

A client that shares only its delays needs no config. A strategy is `Copy`, and its constructors are `const`, so the
client can keep an `Exponential` in a field and limit it for each retry:

```rust,ignore
retry_async(|| self.fetch(path)).delays(self.retry_delays.take(self.max_retries)).run().await
```

## Delays

The delays are any iterator of `Duration`s that states its limit:

- The strategies are `delay::Fixed::of(interval)` and `delay::Exponential::of(first)`. `.capped_at(cap)` caps an
  exponential strategy, and `.with_factor(factor)` grows it by a factor other than 2.
- `.take(n)` allows at most `n` retries, `.take_total(budget)` takes delays while their sum stays within `budget`, and
  `.forever()` states that there is no limit.
- An array, a `Vec` or a range states its limit by its length, so linear delays are `(1..=5).map(|n| n.seconds())`.
  The standard library's adapters, such as `map` and `chain`, keep the limit of the delays they adapt.
- `.jitter(random)` multiplies each delay by a random factor, so that clients that failed together do not retry
  together. With the feature `fastrand`, `delay::random_factor` provides the factors. Limit the delays first, then add
  jitter: `Exponential::of(first).take(n).jitter(random_factor)`.

## Sleeping

A blocking retry puts its thread to sleep, and an async retry sleeps with tokio. `sleep_with(sleep)` sets any other
sleep: one from [`try_again::sleep`][sleep], for futures-timer (any executor, and the browser), async-io (smol) or
embassy, or a closure, such as `|delay| monoio::time::sleep(delay)`. The sleep measures the deadline and the timeouts
too, so all of them follow the sleep's clock. A `RetryConfig` can hold the sleep, so a client sets it once.

Outside a tokio runtime, an async retry without `sleep_with` panics when it is first polled.

## Recipes

The documentation shows complete recipes:

- [Polling][recipe-polling]: waiting until a job is done.
- [`Retry-After`][recipe-retry-after]: waiting as long as a server asks.
- [A retry budget][recipe-budget]: capping the retries of all requests of a client together.
- [Shared settings][RetryConfig]: settings that a client keeps, with closures, in a field.
- [Reconnecting][retry_async_lending]: repairing `&mut` state before each try.
- [A time limit for a blocking retry][retry]: a `when` predicate that reads a clock.
- [Delays of your own][recipe-delays]: Fibonacci delays, decorrelated jitter, and delays in a loop of your own.
- [Testing][recipe-testing]: retries on tokio's paused clock.

## Compared with other crates

This compares try-again 0.3 with [backon] 1.6, [retry][retry-crate] 2.2, [tokio-retry] 0.3, [tokio-retry2] 0.9,
[backoff] 0.4, [tryhard] 0.5 and [again] 0.1. The backoff crate is unmaintained ([RUSTSEC-2025-0012]).

What sets try-again apart:

- **A limit that must be stated.** try-again is the only one of these crates that rejects delays without a stated
  limit at compile time. With retry and tokio-retry, a strategy without `take` retries forever. backon defaults to 3
  retries, and backoff defaults to 15 minutes.
- **Time limits that cancel.** try-again is the only one with a per-try timeout and a deadline that cancel a try in
  progress. The pluggable sleep measures both, so they work on embassy and in the browser. tokio-retry2 and backoff
  check their time limits between tries, and read a clock for them.
- **An error with the whole story.** `RetryError` carries the last failure, the reason, the number of tries and the
  total delay. backon, tokio-retry, tryhard and again return the last error only. The retry crate returns the tries
  and the total delay, without a reason.
- **Any fallible output.** An operation may return a `Result`, an `Option`, or any type that implements `Fallible`.
- **Server-chosen delays everywhere.** `override_next_delay` works for blocking retries too, and an overridden delay
  still uses up one of the retry's delays, so the limit holds. `delay::retry_after` parses the header, HTTP dates
  included.
- **Jitter within the cap.** A delay with `.jitter(random_factor)` never exceeds the cap of its strategy.
- **Few demands.** The crate has no `unsafe` code, and no dependencies without its default features. Delays and sleeps
  need not be `Send`, `Sync` or `'static`.

Where backon is the better fit:

- It has the shortest happy path: `fetch.retry(ExponentialBuilder::default()).await?`.
- It returns the plain error `E`, so `?` works unchanged when that error is an `anyhow::Error`.
- Its jitter needs no feature.
- It works in the browser with its default features.

## Coming from another crate

From backon:

- `with_max_times(n)` → `.take(n)`
- `without_max_times()` → `.forever()`
- `with_total_delay(d)` → `.take_total(d)`
- `with_jitter()` → `.jitter(random_factor)`. backon's own range, which exceeds the cap, is
  `.jitter(|| 1.0 + random_factor())`.
- `notify` → `on_retry`
- `adjust` → `override_next_delay`, with `when` to stop the retry
- `.context(x)` and `RetryableWithContext` → `retry_async_lending`
- a result with the plain error `E` → `.run().map_err(|error| error.failure)`

From backoff:

- `Error::Permanent` → `when`
- `Error::retry_after` → `override_next_delay`
- `max_elapsed_time` → `timeout_total` for an async retry. A blocking retry checks a clock in `when` instead; see
  [`retry`][retry].
- `ExponentialBackoff::default()` is close to
  `Exponential::of(500.milliseconds()).with_factor(1.5).capped_at(1.minutes())`, with
  `.jitter(|| 0.5 + random_factor())` after the limit, and `timeout_total(15.minutes())`.

From tokio-retry or retry:

- `strategy.map(jitter).take(3)` → `.take(3).jitter(random_factor)`: limit first, then jitter.

## Features

- `std` (default): `ThreadSleep`, the default sleep of blocking retries, and `retry_async_lending`.
- `tokio` (default): `TokioSleep`, the default sleep of async retries. Without it, an async retry needs
  `sleep_with(sleep)`.
- `alloc`: `Vec`s, `VecDeque`s and `Box<dyn ExplicitLimit<Item = Duration>>` as delays, without `std`.
- `futures-timer`: `FuturesTimerSleep`, for any executor and the browser.
- `async-io`: `AsyncIoSleep`, for smol.
- `embassy-time`: `EmbassySleep`, for embassy, even without `std`. It tracks embassy-time 0.5. A closure passed to
  `sleep_with` works with any version.
- `fastrand`: `delay::random_factor`, random factors for jitter, seeded from `crypto.getRandomValues` in the browser.
- `retry-after`: `delay::retry_after`, which reads the `Retry-After` header of an HTTP response.

To retry only blocking operations, or to use a runtime other than tokio, turn off the default features:

```toml
[dependencies]
try-again = { version = "0.3", default-features = false, features = ["std"] }
```

In the browser (`wasm32-unknown-unknown`), pass `FuturesTimerSleep` to `sleep_with`. Without `std`, on a
microcontroller, pass `EmbassySleep` or a sleep of your own. The compiler reports a retry that has no sleep.

## MSRV

The MSRV is Rust `1.85`. A patch release never raises it; a raise comes with a new minor version.

## License

Licensed under either [Apache License, Version 2.0][license-apache] or [MIT license][license-mit], at your option.

[crates-badge]: https://img.shields.io/crates/v/try-again.svg
[crates]: https://crates.io/crates/try-again
[docs-badge]: https://docs.rs/try-again/badge.svg
[docs]: https://docs.rs/try-again
[ci-badge]: https://github.com/lpotthast/try-again/actions/workflows/ci.yml/badge.svg
[ci]: https://github.com/lpotthast/try-again/actions/workflows/ci.yml
[coverage-badge]: https://codecov.io/gh/lpotthast/try-again/graph/badge.svg
[coverage]: https://codecov.io/gh/lpotthast/try-again
[msrv-badge]: https://img.shields.io/badge/MSRV-1.85.0-blue.svg
[cargo-toml]: https://github.com/lpotthast/try-again/blob/main/Cargo.toml
[license-badge]: https://img.shields.io/crates/l/try-again.svg
[license-apache]: https://github.com/lpotthast/try-again/blob/main/LICENSE-APACHE
[license-mit]: https://github.com/lpotthast/try-again/blob/main/LICENSE-MIT
[retry]: https://docs.rs/try-again/latest/try_again/fn.retry.html
[retry_async]: https://docs.rs/try-again/latest/try_again/fn.retry_async.html
[retry_async_lending]: https://docs.rs/try-again/latest/try_again/fn.retry_async_lending.html
[Fallible]: https://docs.rs/try-again/latest/try_again/trait.Fallible.html
[RetryError]: https://docs.rs/try-again/latest/try_again/struct.RetryError.html
[TryFailure]: https://docs.rs/try-again/latest/try_again/enum.TryFailure.html
[NoValue]: https://docs.rs/try-again/latest/try_again/struct.NoValue.html
[DurationExt]: https://docs.rs/try-again/latest/try_again/trait.DurationExt.html
[RetryConfig]: https://docs.rs/try-again/latest/try_again/struct.RetryConfig.html
[sleep]: https://docs.rs/try-again/latest/try_again/sleep/index.html
[recipe-polling]: https://docs.rs/try-again/latest/try_again/struct.Retry.html#recipe-polling-until-a-job-is-done
[recipe-retry-after]: https://docs.rs/try-again/latest/try_again/struct.Retry.html#recipe-honoring-retry-after
[recipe-budget]: https://docs.rs/try-again/latest/try_again/struct.Retry.html#recipe-a-retry-budget
[recipe-delays]: https://docs.rs/try-again/latest/try_again/delay/index.html#delays-of-your-own
[recipe-testing]: https://docs.rs/try-again/latest/try_again/sleep/index.html#testing-retries
[backon]: https://crates.io/crates/backon
[retry-crate]: https://crates.io/crates/retry
[tokio-retry]: https://crates.io/crates/tokio-retry
[tokio-retry2]: https://crates.io/crates/tokio-retry2
[backoff]: https://crates.io/crates/backoff
[tryhard]: https://crates.io/crates/tryhard
[again]: https://crates.io/crates/again
[RUSTSEC-2025-0012]: https://rustsec.org/advisories/RUSTSEC-2025-0012.html
