# try-again

Retry **synchronous** or **asynchronous** operations until they no longer can or need to be retried.

Provides `fn retry` for retrying synchronous operations.

Provides `async fn retry_async` for retrying asynchronous operations.

The retried closure may return any type that implements `NeedsRetry`. This trait is already implemented for any `Result` and `Option`, allowing you to retry common fallible outcomes.

A retry strategy is required. The provided `Retry` type provides an implementation supporting

- A maximum number of retries
- A delay between retries being either:
  - None
  - A static delay
  - An exponentially increasing delay

A delay strategy is required and performs the actual delaying between executions of the users closure:

- In the synchronous case: `ThreadSleep {}` can be used, blocking the current thread until the next try should take place.
- In the asynchronous case: `TokioSleep {}` can be used when using the Tokio runtime.

Other delay strategies may be implemented to support async_std or other asynchronous runtimes.

## Synchronous example

```Rust
use try_again::{retry, Delay, Retry, ThreadSleep};

fn some_fallible_operation() -> Result<(), ()> {
    Ok(())
}

let final_outcome = retry(
    Retry {
        max_tries: 5,
        delay: Some(Delay::Static {
            delay: Duration::from_millis(125),
        }),
    },
    ThreadSleep {},
    move || some_fallible_operation(),
);
```

## Asynchronous example

```Rust
use try_again::{retry_async, Delay, Retry, TokioSleep};

async fn some_fallible_operation() -> Result<(), ()> {
    Ok(())
}

let final_outcome = retry_async(
    Retry {
        max_tries: 10,
        delay: Some(Delay::ExponentialBackoff {
            initial_delay: Duration::from_millis(125),
            max_delay: Some(Duration::from_secs(2)),
        }),
    },
    TokioSleep {},
    move || async move {
        some_fallible_operation().await
    },
).await;
```
