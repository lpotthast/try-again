use super::*;

#[test]
fn returns_a_failure_not_worth_another_try_at_once() {
    // Arrange
    let calls = Calls::default();
    let delays = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<Failure>> = retry(|| {
        let call = calls.next();
        if call == 1 {
            Err(Failure::Transient)
        } else {
            Err(Failure::Permanent)
        }
    })
    .when(|failure| *failure == Failure::Transient)
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(delays.sleep())
    .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: Failure::Permanent,
        reason: Rejected,
        tries: 2,
        total_delay: 1.seconds(),
    }));
    assert_that!(calls.get()).is_equal_to(2);
    assert_that!(delays.into_inner()).has_length(1);
}

#[test]
fn the_predicate_may_keep_state() {
    // Arrange
    let mut judged = Vec::new();

    // Act
    let out: Result<(), RetryError<u32>> = retry({
        let mut calls = 0;
        move || {
            calls += 1;
            Err(calls)
        }
    })
    .when(|failure| {
        judged.push(*failure);
        judged.len() < 3
    })
    .delays(Fixed::of(Duration::ZERO).forever())
    .sleep_with(RecordedDelays::default().sleep())
    .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: Duration::ZERO,
    }));
    assert_that!(judged).contains_exactly([1, 2, 3]);
}

#[test]
fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let calls = Calls::default();
    let earlier = Calls::default();

    // Act
    let out = retry(fails_until(&calls, u32::MAX))
        .when(|_failure| {
            earlier.next();
            false
        })
        .when(|failure| *failure < 3)
        .delays(Fixed::of(1.milliseconds()).forever())
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: 2.milliseconds(),
    }));
    assert_that!(earlier.get()).is_equal_to(0);
}
