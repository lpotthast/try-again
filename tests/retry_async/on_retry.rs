use super::*;

#[tokio::test(start_paused = true)]
async fn sees_each_retried_failure_with_its_delay_and_the_number_of_tries() {
    // Arrange
    let calls = Calls::default();
    let seen = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<u32>> = retry_async(async || Err(calls.next()))
        .on_retry(|failure, delay, tries| seen.borrow_mut().push((*failure, delay, tries)))
        .delays(Exponential::of(1.seconds()).take(2))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    // The third failure is the last, which the retry returns instead of retrying.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Exhausted,
        tries: 3,
        total_delay: 3.seconds(),
    }));
    assert_that!(seen.into_inner()).contains_exactly([(1, 1.seconds(), 1), (2, 2.seconds(), 2)]);
}

#[tokio::test(start_paused = true)]
async fn runs_before_a_delay_that_the_deadline_cuts_short() {
    // Arrange
    let seen = Calls::default();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(1.seconds()).await;
        Err(())
    })
    .on_retry(|(), _delay, _tries| {
        seen.next();
    })
    .timeout_total(2500.milliseconds())
    .delays(Fixed::of(2.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The retry cannot count the second that the first try took, so it starts the delay
    // after it, which the deadline cuts short. The hook already saw the failure that the
    // retry returns.
    assert_that!(out.map_err(|error| (error.reason, error.tries))).is_equal_to(Err((Expired, 1)));
    assert_that!(seen.get()).is_equal_to(1);
}

#[tokio::test(start_paused = true)]
async fn receives_the_delay_that_override_next_delay_chose() {
    // Arrange
    let seen = RefCell::new(Vec::new());
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .override_next_delay(|(), delay| delay * 10)
        .on_retry(|(), delay, _tries| seen.borrow_mut().push(delay))
        .delays([1.seconds(), 2.seconds()])
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out.map_err(|error| error.total_delay)).is_equal_to(Err(30.seconds()));
    assert_that!(seen.into_inner()).contains_exactly([10.seconds(), 20.seconds()]);
    assert_that!(start.elapsed()).is_equal_to(30.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let seen = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<()>> = retry_async(async || Err(()))
        .on_retry(|(), _delay, tries| seen.borrow_mut().push(("earlier", tries)))
        .on_retry(|(), _delay, tries| seen.borrow_mut().push(("later", tries)))
        .delays(Fixed::of(1.seconds()).take(2))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out).is_err();
    assert_that!(seen.into_inner()).contains_exactly([("later", 1), ("later", 2)]);
}
