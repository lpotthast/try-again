use super::*;

#[test]
fn replaces_the_scheduled_delay() {
    // Arrange
    let calls = Calls::default();
    let operation = fails_until(&calls, u32::MAX);
    let sleep = RecordedDelays::default();

    // Act
    let out = retry(operation)
        .override_next_delay(|failure, delay| {
            if *failure == 1 {
                10.milliseconds()
            } else {
                delay
            }
        })
        .delays([1.milliseconds(), 2.milliseconds()])
        .sleep_with(sleep.sleep())
        .run();

    // Assert
    // Only the delay after the first failure changes; the second comes from the delays.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Exhausted,
        tries: 3,
        total_delay: 12.milliseconds(),
    }));
    assert_that!(sleep.into_inner()).contains_exactly([10.milliseconds(), 2.milliseconds()]);
}

#[test]
fn the_budget_of_take_total_counts_the_scheduled_delays_not_the_overridden_ones() {
    // Arrange
    let sleep = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<()>> = retry(|| Err(()))
        .override_next_delay(|(), delay| delay * 10)
        .delays(Fixed::of(1.seconds()).take_total(3.seconds()))
        .sleep_with(sleep.sleep())
        .run();

    // Assert
    // Three scheduled delays of 1 second fit into the budget, although the retry waits 10 seconds
    // for each.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: (),
        reason: Exhausted,
        tries: 4,
        total_delay: 30.seconds(),
    }));
    assert_that!(sleep.into_inner()).contains_exactly([10.seconds(); 3]);
}

#[test]
fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let sleep = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<()>> = retry(|| Err(()))
        .override_next_delay(|(), delay| delay * 10)
        .override_next_delay(|(), delay| delay + 1.milliseconds())
        .delays([1.milliseconds(), 2.milliseconds()])
        .sleep_with(sleep.sleep())
        .run();

    // Assert
    assert_that!(out).is_err();
    assert_that!(sleep.into_inner()).contains_exactly([2.milliseconds(), 3.milliseconds()]);
}
