//! Settings that async retries share: `RetryConfig` and `Retry::config`.

use super::*;

#[tokio::test(start_paused = true)]
async fn a_shared_config_brings_its_time_limits() {
    // Arrange
    let config = RetryConfig::new()
        .timeout_total(10.seconds())
        .timeout_each(2.seconds())
        .when(|failure: &TryFailure<u32>| matches!(failure, TryFailure::TimedOut))
        .delays(Fixed::of(1.seconds()).forever());
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        if call == 1 {
            tokio::time::sleep(1.minutes()).await;
        }
        Err(call)
    })
    .config(&config)
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The first try times out after 2 seconds, and the second fails after the delay.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(2),
        reason: Rejected,
        tries: 2,
        total_delay: 1.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(3.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_shared_config_brings_its_sleep() {
    // Arrange
    let waits = Calls::default();
    let config = RetryConfig::new()
        .timeout_total(5.seconds())
        .delays(Fixed::of(2.seconds()).forever())
        .sleep_with(|delay: Duration| {
            waits.next();
            tokio_clock(delay)
        });
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> =
        retry_async(async || Err(())).config(&config).run().await;

    // Assert
    // The sleep measures the deadline and both delays: the tries run at 0, 2 and 4 seconds, and
    // the next delay would end after the deadline.
    assert_that!(out.map_err(|error| (error.reason, error.tries))).is_equal_to(Err((Expired, 3)));
    assert_that!(waits.get()).is_equal_to(3);
    assert_that!(start.elapsed()).is_equal_to(4.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_sleep_after_a_shared_config_replaces_its_sleep() {
    // Arrange
    let shared_waits = Calls::default();
    let config = RetryConfig::new()
        .timeout_total(5.seconds())
        .delays(Fixed::of(2.seconds()).forever())
        .sleep_with(|delay: Duration| {
            shared_waits.next();
            tokio_clock(delay)
        });
    let waits = Calls::default();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .config(&config)
        .sleep_with(|delay| {
            waits.next();
            tokio_clock(delay)
        })
        .run()
        .await;

    // Assert
    // The sleep of the retry measures the deadline and both delays of the config.
    assert_that!(out.map_err(|error| (error.reason, error.tries))).is_equal_to(Err((Expired, 3)));
    assert_that!(waits.get()).is_equal_to(3);
    assert_that!(shared_waits.get()).is_equal_to(0);
}

#[tokio::test(start_paused = true)]
async fn time_limits_after_a_shared_config_replace_its_time_limits() {
    // Arrange
    let config = RetryConfig::new()
        .timeout_total(3.seconds())
        .timeout_each(1.seconds())
        .delays(Fixed::of(1.seconds()).forever());
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(1.minutes()).await;
        Err(())
    })
    .config(&config)
    .timeout_total(10.seconds())
    .timeout_each(4.seconds())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The tries start at 0 and 5 seconds and time out after 4 seconds each, with the delay of
    // the config between them. The delay after the second ends at the deadline of 10 seconds.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::TimedOut,
        reason: Expired,
        tries: 2,
        total_delay: 2.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(10.seconds());
}

#[tokio::test(start_paused = true)]
async fn each_retry_gets_its_own_deadline_from_a_shared_config() {
    // Arrange
    let config = RetryConfig::new()
        .timeout_total(5.seconds())
        .delays(Fixed::of(2.seconds()).forever());
    let start = Instant::now();

    // Act
    let first: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .config(&config)
        .sleep_with(tokio_clock)
        .run()
        .await;
    let second: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .config(&config)
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    // Each retry waits 2 delays, as a third would end after its deadline.
    assert_that!(first.map_err(|error| error.tries)).is_equal_to(Err(3));
    assert_that!(second.map_err(|error| error.tries)).is_equal_to(Err(3));
    assert_that!(start.elapsed()).is_equal_to(8.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_retry_with_a_shared_config_can_be_spawned() {
    // Arrange
    let config = RetryConfig::new()
        .timeout_each(5.seconds())
        .timeout_total(30.seconds())
        .delays(Exponential::of(1.seconds()).forever());

    // Act
    let task = tokio::spawn(async move {
        let tries = AtomicU32::new(0);
        retry_async(async || connect(&tries).await)
            .config(&config)
            .sleep_with(tokio_clock)
            .run()
            .await
    });
    let out = task.await.expect("the task must not panic");

    // Assert
    assert_that!(out).ok().is_equal_to(3);
}

#[test]
fn shows_its_settings_in_debug() {
    let retry = retry_async(async || Err::<(), ()>(()))
        .timeout_total(5.seconds())
        .delays([1.seconds()]);

    let debug = format!("{retry:?}");

    assert_that!(debug).is_equal_to(
        "Retry { settings: RetryConfig { delays: IntoIter([1s]), timeout_total: 5s, \
         timeout_each: (), .. }, .. }",
    );
}
