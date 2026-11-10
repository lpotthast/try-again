use super::*;

#[tokio::test(start_paused = true)]
async fn replaces_the_scheduled_delay() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .override_next_delay(|(), delay| delay * 2)
        .delays(Fixed::of(1.seconds()).take(2))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out.map_err(|error| error.total_delay)).is_equal_to(Err(4.seconds()));
    assert_that!(start.elapsed()).is_equal_to(4.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .override_next_delay(|(), delay| delay * 10)
        .override_next_delay(|(), delay| delay + 2.seconds())
        .delays(Fixed::of(1.seconds()).take(2))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out.map_err(|error| error.total_delay)).is_equal_to(Err(6.seconds()));
    assert_that!(start.elapsed()).is_equal_to(6.seconds());
}

#[tokio::test(start_paused = true)]
async fn receives_the_try_failure_with_a_timeout_for_each_try() {
    // Arrange
    let calls = Calls::default();
    let seen = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        if call == 1 {
            tokio::time::sleep(1.minutes()).await;
        }
        Err(call)
    })
    .timeout_each(5.seconds())
    .override_next_delay(|failure, delay| {
        seen.borrow_mut().push(*failure);
        if matches!(failure, TryFailure::TimedOut) {
            Duration::ZERO
        } else {
            delay
        }
    })
    .delays(Fixed::of(1.seconds()).take(2))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // A timed-out try is retried at once, a failed one after the scheduled delay.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(3),
        reason: Exhausted,
        tries: 3,
        total_delay: 1.seconds(),
    }));
    assert_that!(seen.into_inner()).contains_exactly([TryFailure::TimedOut, TryFailure::Failed(2)]);
}

#[tokio::test(start_paused = true)]
async fn gives_up_at_once_when_an_overridden_delay_would_pass_the_deadline() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .override_next_delay(|(), _delay| Duration::MAX)
        .timeout_total(30.seconds())
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(()),
        reason: Expired,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
    assert_that!(start.elapsed()).is_equal_to(Duration::ZERO);
}

#[cfg(feature = "retry-after")]
#[tokio::test(start_paused = true)]
async fn waits_as_long_as_a_retry_after_header_asks() {
    // Arrange
    use std::time::SystemTime;
    use try_again::delay::retry_after;

    // Sun, 06 Nov 1994 08:48:37 GMT
    let now = SystemTime::UNIX_EPOCH + 784_111_717.seconds();
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<u32, RetryError<&str>> = retry_async(async || match calls.next() {
        1 => Err("120"),
        2 => Err("Sun, 06 Nov 1994 08:49:37 GMT"),
        call => Ok(call),
    })
    .override_next_delay(|header, delay| retry_after(header, now).unwrap_or(delay))
    .delays(Fixed::of(1.seconds()).take(5))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // Two minutes, then one until the date.
    assert_that!(out).is_equal_to(Ok(3));
    assert_that!(start.elapsed()).is_equal_to(3.minutes());
}
