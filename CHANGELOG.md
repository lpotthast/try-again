# Changelog

## 0.3.0 (unreleased)

A redesign around a retry builder and iterators of delays. Every retry states its limit, with `.take(n)`,
`.take_total(budget)`, `.forever()`, or a finite source such as an array. An async retry can have a deadline, a hard
time limit from its first try, and a timeout for each try. A `RetryConfig` shares settings between retries.

### Migrating from 0.2

```rust,ignore
// 0.2
let out = retry(op).delayed_by(delay::ExponentialBackoff::of_initial_delay(a).capped_at(b).take(n));
let out = retry_async(op).delayed_by(delay::Fixed::of(250.millis()).take(n)).await;
let out = retry_with_options(op, RetryOptions { delay_strategy, delay_executor });
// 0.3
let out = retry(op).delays(delay::Exponential::of(a).capped_at(b).take(n)).run();
let out = retry_async(op).delays(delay::Fixed::of(250.milliseconds()).take(n)).run().await;
let out = retry(op).delays(strategy).sleep_with(sleep).run();
```

A retry now returns `Result<T, RetryError<E>>` instead of the operation's last output.
`.run().map_err(|error| error.failure)` gives a `Result<T, E>` back, and `.run().ok()` gives an `Option<T>` back. Each
breaking change below names its replacement.

### Added

- `Retry`, the one builder of blocking and async retries. Its settings can be set in any order, with two exceptions:
  `config` must be called first, and `timeout_each` must be called before `when`, `override_next_delay` and
  `on_retry`. Setting something again replaces it.
- `sleep_with(sleep)`, which sets the sleep that a retry waits with. By default, a blocking retry puts its thread to
  sleep, and an async retry sleeps with tokio.
- `when(predicate)`, which retries only the failures for which the predicate returns `true`.
- `override_next_delay(override)`, which replaces the delay after a failure, for example with an HTTP `Retry-After`.
- `on_retry(hook)`, which runs before each retry, for example to log a warning. The hook receives the failure, the
  delay, and the number of tries so far.
- `retry_async(..).timeout_total(timeout)`, a deadline for an async retry, and `retry_async(..).timeout_each(timeout)`,
  a timeout for each try. With either, the retry's failure is a `TryFailure<E>`. It displays as the operation's
  failure, or as a try that timed out.
- `RetryError<E>`, the error of a retry that gave up: the last `failure`, the `reason` (`StopReason`), the number of
  `tries` and the `total_delay`.
- `NoValue`, the failure of an operation that returned `None`. It implements `Error`, so `?` works on a retry of an
  operation that returns an `Option`.
- `RetryConfig`, the settings of a retry without its operation, its sleep included, and `Retry::config(&shared)`,
  which applies them. Retries can share a `RetryConfig`, and a client can keep it in a field. The settings after
  `config` add to the config, or replace what it set.
- `retry_async_lending(&mut state, operation)` (feature `std`), for async operations that need `&mut` state. It will
  be deprecated once stable Rust can show that a retry of an `async` closure that mutates its captures is `Send`.
- The delays of a retry are any `IntoIterator<Item = Duration>` that states its limit. The trait
  `delay::ExplicitLimit` checks that. Arrays, `Vec`s and ranges with an end state their limit by their length, so
  linear delays are `(1..=5).map(|n| n.seconds())`.
- The trait `delay::DelaysExt`, which adds `forever()`, `take_total(budget)` and `jitter(random)` to any iterator of
  delays, and `delay::random_factor()` (feature `fastrand`), which gives random factors for jitter.
- `delay::retry_after(value, now)` (feature `retry-after`), which reads the `Retry-After` header of an HTTP response,
  in seconds or as an HTTP date.
- `delay::Exponential::with_factor(factor)`, `delay::Exponential::delay_after(failures)`, and the getter
  `delay::Exponential::factor()`.
- The public module `sleep`, with the traits `Sleep` and `AsyncSleep`, and the sleeps `FuturesTimerSleep` (feature
  `futures-timer`, any executor and the browser), `AsyncIoSleep` (feature `async-io`, smol) and `EmbassySleep`
  (feature `embassy-time`). A closure is a sleep too: an `Fn(Duration)` for a blocking retry, and a closure that
  returns a future for an async one.
- Support for targets without `std`, and the features `std` and `alloc`.
- `DurationExt::minutes()` and `DurationExt::hours()`. For huge numbers, they return `Duration::MAX`.

### Changed

- **Breaking:** `retry(op).delayed_by(strategy)` is now `retry(op).delays(strategy).run()`, and likewise for
  `retry_async`, which is awaited after `run()`.
- **Breaking:** a retry returns `Result<T, RetryError<E>>` for an operation that returns `Result<T, E>`, and
  `Result<T, RetryError<NoValue>>` for one that returns `Option<T>`. 0.2 returned the operation's last output.
- **Breaking:** `NeedsRetry` is now `Fallible`: `impl NeedsRetry for T { fn needs_retry(&self) -> bool }` becomes
  `impl Fallible for T { type Success; type Failure; fn into_result(self) -> Result<..> }`.
- **Breaking:** `IntoStdDuration` is now `DurationExt`, which is sealed. Its methods are spelled out: `nanos()`,
  `micros()`, `millis()` and `secs()` are now `nanoseconds()`, `microseconds()`, `milliseconds()` and `seconds()`.
- **Breaking:** `delay::ExponentialBackoff::of_initial_delay(a)` is now `delay::Exponential::of(a)`, which is uncapped
  until `.capped_at(b)`. `uncapped()` is gone, and `ExponentialBackoffWithCap` merged into `Exponential`. `of` and
  `capped_at` panic for a zero duration, whose delays would all be zero; use `Fixed` for immediate retries.
  `Exponential` no longer implements `Eq`, as its factor is an `f64`.
- **Breaking:** `delay::Fixed` and `delay::Exponential` are strategies, not iterators: `take(n)`,
  `take_total(budget)` and `forever()` return their delays. Their constructors take a `Duration` instead of
  `impl Into<Duration>`, and are `const`.
- **Breaking:** the fields of `Fixed` and of the exponential backoffs are private. Use the getters instead:
  `Fixed::delay` becomes `interval()`, `initial_delay` becomes `first()`, and `max_delay: Option<Duration>` becomes
  `cap()`, which is `Duration::MAX` for an uncapped strategy. The iteration state of `ExponentialBackoffWithCap`,
  `last_delay` and `first`, has no replacement.
- **Breaking:** a custom delay strategy is an `Iterator<Item = Duration>` instead of a `DelayStrategy<Duration>`. A
  retry accepts it once it states its limit: implement `delay::ExplicitLimit` for a finite one, or limit it with
  `.take(n)`, `.take_total(budget)` or `.forever()`.
- **Breaking:** `DelayExecutor` and `AsyncDelayExecutor` are now `sleep::Sleep` and `sleep::AsyncSleep`, and their
  method `delay_by` is now `sleep`. They lost their `Debug` supertrait and their type parameter: a delay is now always
  a `Duration`. `AsyncSleep` names the future of a wait as its associated type `Future`, so code that is generic over
  the sleep can require that future to be `Send`. `ThreadSleep` and `TokioSleep` moved from the module
  `delay_executor` to the module `sleep`.
- **Breaking:** the operation of `retry_async` is a closure that returns a future (`FnMut() -> impl Future`) instead of
  an `AsyncFn`. The retry's future is now `Send` when the operation's futures are, even for closures that borrow. A
  plain closure can mutate its captures before it returns the future, for example to count tries. An `async move`
  closure whose future borrows what the closure owns, such as `async move || get(&url).await`, no longer compiles:
  borrow the value outside the closure (`let url = &url;`, then `async || get(url).await`).
- **Breaking:** the feature `async-tokio` is now `tokio`, and the feature `async` is gone. With
  `default-features = false`, add the new feature `std` to keep `ThreadSleep` as the default sleep of blocking
  retries; without it, a blocking retry needs `sleep_with(..)`.
- **Breaking (behavior):** an exponential strategy caps its first delay too; 0.2 used the initial delay as given.
  Delays that grow beyond `Duration::MAX` now saturate there; 0.2 panicked on that overflow.
- **Breaking (behavior):** a retry no longer logs, and the `tracing` dependency is gone. 0.2 logged a retry that ran
  out of retries at `error` level: log in `on_retry`, or log the `RetryError` when the retry gives up.
- **Breaking (behavior):** outside a tokio runtime, an async retry with the default sleep panics when it is first
  polled. 0.2 panicked only at the first delay, which a retry whose first try succeeds never reaches.
- The operation of `retry` is `FnMut` instead of `Fn`, and the outputs of `retry` and `retry_async` no longer need
  `Debug`.
- A retry without a deadline drops a failure before it waits, so its future is `Send` even when the failure is not.
- `ThreadSleep` is not built for `wasm32-unknown-unknown`, where it would panic; a blocking retry there needs
  `sleep_with(..)`.
- The default tokio sleep is not available for `wasm32-unknown-unknown` either, where tokio has no timers. An async
  retry there needs `sleep_with(..)`, for example `sleep_with(FuturesTimerSleep)`, and does not compile without it.
- All dependencies are declared with `default-features = false`. The lower bound of tokio is now 1.7; it was 1.0.

### Removed

- **Breaking:** `retry_with_options`, `retry_async_with_options`, `RetryOptions`, `RetryAsyncOptions`,
  `NeedsDelayStrategy` and `AsyncNeedsDelayStrategy`. `retry_with_options(op, RetryOptions { .. })` becomes
  `retry(op).delays(strategy).sleep_with(sleep).run()`, and likewise for `retry_async`.
- **Breaking:** the trait `DelayStrategy` and the module `delay_strategy`; see above.
- **Breaking:** `delay::None`. Use `delay::Fixed::of(Duration::ZERO).take(n)`.
- **Breaking:** the type alias `StdDuration`. Use `std::time::Duration`.
- **Breaking:** `Fallible` (formerly `NeedsRetry`) is no longer implemented for `ExitStatus`. `Command::status()`
  returns an `io::Result<ExitStatus>`, and the `Result` impl treated an unsuccessful exit as a success, so the impl
  rarely helped. Turn an unsuccessful status into an error in the operation instead.

## Earlier versions

Versions before 0.3 have no changelog. Their code is in the repository under the [tags] `v0.1.0` to `v0.2.2`.

[tags]: https://github.com/lpotthast/try-again/tags
