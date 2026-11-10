//! The sleeps of this crate, behind their features. `run()` sleeps with `TokioSleep`, on tokio's
//! paused clock. The sleeps of other runtimes run on real time, which tokio's paused clock does not
//! control, so their waits are short.

use super::*;

#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn sleeps_with_tokio_by_default() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .delays(Fixed::of(1.seconds()).take(2))
        .run()
        .await;

    // Assert
    assert_that!(out).is_err();
    assert_that!(start.elapsed()).is_equal_to(2.seconds());
}

#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn measures_the_deadline_with_tokio() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .timeout_total(5.seconds())
        .delays(Fixed::of(2.seconds()).forever())
        .run()
        .await;

    // Assert
    // A third delay would end after the deadline.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(()),
        reason: Expired,
        tries: 3,
        total_delay: 4.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(4.seconds());
}

#[cfg(feature = "tokio")]
#[tokio::test(start_paused = true)]
async fn a_retry_that_sleeps_with_tokio_can_be_spawned() {
    // Arrange
    let task = tokio::spawn(async {
        let tries = AtomicU32::new(0);
        retry_async(async || connect(&tries).await)
            .timeout_total(30.seconds())
            .delays(Exponential::of(1.seconds()).forever())
            .run()
            .await
    });

    // Act
    let out = task.await.expect("the task must not panic");

    // Assert
    assert_that!(out).ok().is_equal_to(3);
}

#[cfg(feature = "tokio")]
#[test]
#[should_panic(expected = "there is no reactor running")]
fn the_default_sleep_panics_at_the_first_poll_outside_a_tokio_runtime() {
    use std::task::{Context, Waker};

    // The first try would succeed, so the retry would never wait for a delay.
    let run = std::pin::pin!(
        retry_async(async || Ok::<u8, ()>(1))
            .delays(Fixed::of(1.seconds()).take(2))
            .run()
    );

    let _ = run.poll(&mut Context::from_waker(Waker::noop()));
}

/// Retries twice, 10 ms apart, with `sleep`, and returns how long the retry took.
#[cfg(any(
    feature = "futures-timer",
    feature = "async-io",
    feature = "embassy-time"
))]
async fn retry_twice(sleep: impl AsyncSleep) -> std::time::Duration {
    let start = std::time::Instant::now();
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .delays(Fixed::of(10.milliseconds()).take(2))
        .sleep_with(sleep)
        .run()
        .await;
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: (),
        reason: Exhausted,
        tries: 3,
        total_delay: 20.milliseconds(),
    }));
    start.elapsed()
}

/// Runs a try that never completes, with a deadline of 10 ms that `sleep` measures, and returns
/// how long the retry took.
#[cfg(any(
    feature = "futures-timer",
    feature = "async-io",
    feature = "embassy-time"
))]
async fn outlast_the_deadline(sleep: impl AsyncSleep) -> std::time::Duration {
    let start = std::time::Instant::now();
    let out: Result<(), RetryError<TryFailure<()>>> =
        retry_async(std::future::pending::<Result<(), ()>>)
            .timeout_total(10.milliseconds())
            .delays(Fixed::of(1.milliseconds()).forever())
            .sleep_with(sleep)
            .run()
            .await;
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::TimedOut,
        reason: Expired,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
    start.elapsed()
}

#[cfg(feature = "futures-timer")]
#[tokio::test]
async fn sleeps_with_futures_timer() {
    let elapsed = retry_twice(try_again::sleep::FuturesTimerSleep).await;

    assert_that!(elapsed).is_greater_or_equal_to(20.milliseconds());
}

#[cfg(feature = "futures-timer")]
#[tokio::test]
async fn measures_the_deadline_with_futures_timer() {
    let elapsed = outlast_the_deadline(try_again::sleep::FuturesTimerSleep).await;

    // A sleep that completes at once would end the retry in the same way, but sooner.
    assert_that!(elapsed).is_greater_or_equal_to(10.milliseconds());
}

#[cfg(feature = "futures-timer")]
#[tokio::test]
async fn a_retry_that_sleeps_with_futures_timer_can_be_spawned() {
    let task = tokio::spawn(retry_twice(try_again::sleep::FuturesTimerSleep));

    assert_that!(task.await).is_ok();
}

#[cfg(feature = "async-io")]
#[test]
fn sleeps_with_async_io() {
    let elapsed = async_io::block_on(retry_twice(try_again::sleep::AsyncIoSleep));

    assert_that!(elapsed).is_greater_or_equal_to(20.milliseconds());
}

#[cfg(feature = "async-io")]
#[test]
fn measures_the_deadline_with_async_io() {
    let elapsed = async_io::block_on(outlast_the_deadline(try_again::sleep::AsyncIoSleep));

    assert_that!(elapsed).is_greater_or_equal_to(10.milliseconds());
}

#[cfg(feature = "async-io")]
#[tokio::test]
async fn a_retry_that_sleeps_with_async_io_can_be_spawned() {
    let task = tokio::spawn(retry_twice(try_again::sleep::AsyncIoSleep));

    assert_that!(task.await).is_ok();
}

#[cfg(feature = "async-io")]
#[tokio::test]
async fn a_delay_beyond_async_io_s_clock_does_not_end_at_once() {
    let wait = try_again::sleep::AsyncIoSleep.sleep(Duration::MAX);

    let out = tokio::time::timeout(10.milliseconds(), wait).await;

    assert_that!(out).is_err();
}

#[cfg(feature = "embassy-time")]
#[tokio::test]
async fn sleeps_with_embassy() {
    let elapsed = retry_twice(try_again::sleep::EmbassySleep).await;

    assert_that!(elapsed).is_greater_or_equal_to(20.milliseconds());
}

#[cfg(feature = "embassy-time")]
#[tokio::test]
async fn measures_the_deadline_with_embassy() {
    let elapsed = outlast_the_deadline(try_again::sleep::EmbassySleep).await;

    assert_that!(elapsed).is_greater_or_equal_to(10.milliseconds());
}

#[cfg(feature = "embassy-time")]
#[tokio::test]
async fn a_retry_that_sleeps_with_embassy_can_be_spawned() {
    let task = tokio::spawn(retry_twice(try_again::sleep::EmbassySleep));

    assert_that!(task.await).is_ok();
}

#[cfg(feature = "embassy-time")]
#[tokio::test]
async fn a_delay_beyond_embassy_s_clock_waits_without_panicking() {
    let wait = try_again::sleep::EmbassySleep.sleep(Duration::MAX);

    let out = tokio::time::timeout(10.milliseconds(), wait).await;

    assert_that!(out).is_err();
}

#[cfg(feature = "futures-timer")]
#[tokio::test]
async fn a_delay_beyond_futures_timer_s_longest_wait_does_not_end_at_once() {
    let wait = try_again::sleep::FuturesTimerSleep.sleep(Duration::MAX);

    let out = tokio::time::timeout(10.milliseconds(), wait).await;

    assert_that!(out).is_err();
}
