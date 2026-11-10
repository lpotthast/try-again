use super::*;

#[tokio::test(start_paused = true)]
async fn a_delay_starts_when_the_try_ends() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<u32, RetryError<()>> = retry_async(async || {
        tokio::time::sleep(2.seconds()).await;
        let call = calls.next();
        if call < 3 { Err(()) } else { Ok(call) }
    })
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // Three tries of 2 seconds each, with 1 second between them.
    assert_that!(out).is_equal_to(Ok(3));
    assert_that!(start.elapsed()).is_equal_to(8.seconds());
}

#[tokio::test(start_paused = true)]
async fn waits_its_delays_on_tokio_s_clock() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<u32, RetryError<()>> = retry_async(async || {
        let call = calls.next();
        if call < 5 { Err(()) } else { Ok(call) }
    })
    .delays(Exponential::of(1.seconds()).capped_at(4.seconds()).take(10))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The four failures wait 1, 2, 4 and 4 seconds.
    assert_that!(out).is_equal_to(Ok(5));
    assert_that!(start.elapsed()).is_equal_to(11.seconds());
}

#[tokio::test]
async fn sleeps_with_the_sleep_that_the_caller_hands_it() {
    // Arrange
    let delays = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .delays(Exponential::of(100.milliseconds()).take(3))
        .sleep_with(delays.async_sleep())
        .run()
        .await;

    // Assert
    assert_that!(out).is_err();
    assert_that!(delays.into_inner()).contains_exactly([
        100.milliseconds(),
        200.milliseconds(),
        400.milliseconds(),
    ]);
}

#[tokio::test]
async fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let delays = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .delays(Fixed::of(1.seconds()).take(5))
        .delays([2.milliseconds(), 3.milliseconds()])
        .sleep_with(delays.async_sleep())
        .run()
        .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: (),
        reason: Exhausted,
        tries: 3,
        total_delay: 5.milliseconds(),
    }));
    assert_that!(delays.into_inner()).contains_exactly([2.milliseconds(), 3.milliseconds()]);
}

#[tokio::test]
async fn a_later_call_of_sleep_with_replaces_the_earlier_sleep() {
    // Arrange
    let earlier = RecordedDelays::default();
    let later = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .sleep_with(earlier.async_sleep())
        .sleep_with(later.async_sleep())
        .delays([2.milliseconds(), 3.milliseconds()])
        .run()
        .await;

    // Assert
    assert_that!(out).is_err();
    assert_that!(earlier.into_inner()).is_empty();
    assert_that!(later.into_inner()).contains_exactly([2.milliseconds(), 3.milliseconds()]);
}

#[tokio::test(start_paused = true)]
async fn dropping_the_future_stops_a_retry_forever() {
    // Arrange
    let calls = Calls::default();

    // Act
    let outcome = tokio::time::timeout(
        9500.milliseconds(),
        retry_async(async || {
            calls.next();
            Err::<(), ()>(())
        })
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run(),
    )
    .await;

    // Assert
    // The tries run at 0 to 9 seconds, and the timeout drops the retry while it waits after the
    // tenth try.
    assert_that!(outcome).is_err();
    assert_that!(calls.get()).is_equal_to(10);
}

#[tokio::test]
async fn dropping_the_future_stops_a_retry_whose_sleep_never_waits() {
    // Act
    let outcome = tokio::time::timeout(
        20.milliseconds(),
        retry_async(async || Err::<(), ()>(()))
            // Bounded, so that the test fails rather than hangs if the retry never yields.
            .delays(Fixed::of(Duration::ZERO).take(1_000_000))
            .sleep_with(eager_zero)
            .run(),
    )
    .await;

    // Assert
    // The retry yields once per retry, so the timeout fires although no try or delay waits.
    assert_that!(outcome).is_err();
}
