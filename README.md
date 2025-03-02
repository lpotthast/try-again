# try-again

Retry **synchronous** or **asynchronous** operations until they no longer can or need to be retried.

Provides `fn` `retry` for retrying synchronous operations.

Provides `async fn` `retry_async` for retrying asynchronous operations.

Supports closures and function pointers.

The retried operation may return any type that implements `NeedsRetry`.
This trait is already implemented for `Result`, `Option` and `ExitStatus`, allowing you to retry common fallible
outcomes.

## Synchronous example

```rust
use assertr::prelude::*;
use try_again::{delay, retry, IntoStdDuration};

async fn do_smth() {
    fn fallible_operation() -> Result<(), ()> {
        Ok(())
    }

    let final_outcome = retry(fallible_operation)
        .delayed_by(delay::Fixed::of(125.millis()).take(5));

    assert_that(final_outcome).is_ok();
}
```

## Asynchronous example

```rust
use assertr::prelude::*;
use try_again::{delay, retry_async, IntoStdDuration};

async fn do_smth() {
    async fn fallible_operation() -> Result<(), ()> {
        Ok(())
    }

    let final_outcome = retry_async(fallible_operation)
        .delayed_by(delay::ExponentialBackoff::of_initial_delay(125.millis()).capped_at(2.secs()).take(10))
        .await;

    assert_that(final_outcome).is_ok();
}
```

## Details

### Delay strategies

`delayed_by` accepts a delay strategy. The `delay` module provides the following implementations

- `None`: No delay is applied.
- `Fixed`: A static delay.
- `ExponentialBackoff`: An exponentially increasing delay

All work with `std::time::Duration`, re-exposed as `StdDuration`. The `IntoStdDuration` can be used for a fluent syntax
when defining durations, like in

    use try_again::{delay, IntoStdDuration};

    delay::Fixed::of(250.millis())

### Delay executors

The standard `retry` and `retry_async` functions have the following default behavior:

- `retry` puts the current thread to sleep between retries (through the provided `ThreadSleep` executor).
- `retry_async` instructs the tokio runtime to sleep between retries (through the provided `TokioSleep` executor,
  requires the
  `async-tokio` feature (enabled by default)).

The `retry_with_options` and `retry_async_with_options` functions can be used to overwrite the standard behavior
with any executor type implementing the `DelayExecutor` trait.

That way, support for `async_std` or other asynchronous runtimes could be provided.

## MSRV

- As of 0.1.0, the MSRV is `1.56.0`
- As of 0.2.0, the MSRV is `1.85.0`
