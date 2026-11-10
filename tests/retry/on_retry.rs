use super::*;

#[test]
fn sees_each_retried_failure_with_its_delay_and_the_number_of_tries() {
    // Arrange
    let calls = Calls::default();
    let operation = fails_until(&calls, u32::MAX);
    let seen = RefCell::new(Vec::new());

    // Act
    let out = retry(operation)
        .on_retry(|failure, delay, tries| seen.borrow_mut().push((*failure, delay, tries)))
        .delays([1.milliseconds(), 2.milliseconds()])
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    // The third failure is the last, which the retry returns instead of retrying.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Exhausted,
        tries: 3,
        total_delay: 3.milliseconds(),
    }));
    assert_that!(seen.into_inner())
        .contains_exactly([(1, 1.milliseconds(), 1), (2, 2.milliseconds(), 2)]);
}

#[test]
fn receives_the_delay_that_override_next_delay_chose() {
    // Arrange
    let seen = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<()>> = retry(|| Err(()))
        .override_next_delay(|(), delay| delay * 10)
        .on_retry(|(), delay, _tries| seen.borrow_mut().push(delay))
        .delays([1.milliseconds(), 2.milliseconds()])
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out.map_err(|error| error.total_delay)).is_equal_to(Err(30.milliseconds()));
    assert_that!(seen.into_inner()).contains_exactly([10.milliseconds(), 20.milliseconds()]);
}

#[test]
fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let seen = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<()>> = retry(|| Err(()))
        .on_retry(|(), _delay, tries| seen.borrow_mut().push(("earlier", tries)))
        .on_retry(|(), _delay, tries| seen.borrow_mut().push(("later", tries)))
        .delays([Duration::ZERO; 2])
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out).is_err();
    assert_that!(seen.into_inner()).contains_exactly([("later", 1), ("later", 2)]);
}
