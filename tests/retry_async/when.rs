use super::*;

#[tokio::test(start_paused = true)]
async fn returns_a_failure_not_worth_another_try_at_once() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<&str>> = retry_async(async || {
        calls.next();
        Err("permanent")
    })
    .when(|failure| *failure == "transient")
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: "permanent",
        reason: Rejected,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
    assert_that!(calls.get()).is_equal_to(1);
    assert_that!(start.elapsed()).is_equal_to(Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let calls = Calls::default();
    let earlier = Calls::default();

    // Act
    let out: Result<(), RetryError<u32>> = retry_async(async || Err(calls.next()))
        .when(|_failure| {
            earlier.next();
            false
        })
        .when(|failure| *failure < 3)
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: 2.seconds(),
    }));
    assert_that!(earlier.get()).is_equal_to(0);
}
