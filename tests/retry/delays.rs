use super::*;
use try_again::delay::ExplicitLimit;

#[test]
fn waits_its_delays_between_tries() {
    // Arrange
    let calls = Calls::default();
    let operation = fails_until(&calls, 6);
    let delays = RecordedDelays::default();

    // Act
    let out = retry(operation)
        .delays(
            Exponential::of(1.seconds())
                .capped_at(8.seconds())
                .forever(),
        )
        .sleep_with(delays.sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Ok(6));
    assert_that!(delays.into_inner()).contains_exactly([
        1.seconds(),
        2.seconds(),
        4.seconds(),
        8.seconds(),
        8.seconds(),
    ]);
}

#[test]
fn take_allows_that_many_retries_after_the_first_try() {
    // Arrange
    let calls = Calls::default();
    let operation = fails_until(&calls, u32::MAX);
    let delays = RecordedDelays::default();

    // Act
    let out = retry(operation)
        .delays(Fixed::of(50.milliseconds()).take(3))
        .sleep_with(delays.sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 4,
        reason: Exhausted,
        tries: 4,
        total_delay: 150.milliseconds(),
    }));
    assert_that!(calls.get()).is_equal_to(4);
    assert_that!(delays.into_inner()).has_length(3);
}

#[test]
fn no_delays_try_once() {
    // Arrange
    let calls = Calls::default();
    let operation = fails_until(&calls, u32::MAX);

    // Act
    let out = retry(operation)
        .delays([])
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 1,
        reason: Exhausted,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
    assert_that!(calls.get()).is_equal_to(1);
}

/// The delays that a retry of an operation that always fails waits.
fn waited<D>(delays: D) -> Vec<Duration>
where
    D: IntoIterator<Item = Duration>,
    D::IntoIter: ExplicitLimit,
{
    let recorded = RecordedDelays::default();
    let _ = retry(|| Err::<(), ()>(()))
        .delays(delays)
        .sleep_with(recorded.sleep())
        .run();
    recorded.into_inner()
}

#[test]
fn waits_the_delays_of_any_source_that_states_its_limit() {
    let adapted = std::iter::once(Duration::ZERO).chain(
        Exponential::of(1.seconds())
            .capped_at(4.seconds())
            .take(3)
            .map(|delay| delay * 2),
    );

    assert_that!(waited([10.milliseconds(), 5.seconds()]))
        .is_equal_to(vec![10.milliseconds(), 5.seconds()]);
    // A `Vec` states its limit only with the feature `alloc`.
    #[cfg(feature = "alloc")]
    assert_that!(waited(vec![3.milliseconds(), 7.milliseconds()]))
        .is_equal_to(vec![3.milliseconds(), 7.milliseconds()]);
    assert_that!(waited(adapted)).is_equal_to(vec![
        Duration::ZERO,
        2.seconds(),
        4.seconds(),
        8.seconds(),
    ]);
    // `forever` over finite delays ends when they end.
    assert_that!(waited([1.milliseconds(); 2].forever())).is_equal_to(vec![1.milliseconds(); 2]);
}

#[test]
fn waits_the_delays_of_a_range_mapped_to_durations() {
    assert_that!(waited((1..=3u64).map(|n| (n * 100).milliseconds()))).is_equal_to(vec![
        100.milliseconds(),
        200.milliseconds(),
        300.milliseconds(),
    ]);
    assert_that!(waited((1..3u64).map(|n| (n * 100).milliseconds())))
        .is_equal_to(vec![100.milliseconds(), 200.milliseconds()]);
}

#[test]
fn borrowed_delays_are_shared_between_retries() {
    // Arrange
    let mut budget = Fixed::of(Duration::ZERO).take(4);
    let first_calls = Calls::default();
    let second_calls = Calls::default();

    // Act
    let first: Result<(), RetryError<()>> = retry(|| {
        first_calls.next();
        Err(())
    })
    .delays(&mut budget)
    .sleep_with(RecordedDelays::default().sleep())
    .run();
    let second: Result<(), RetryError<()>> = retry(|| {
        second_calls.next();
        Err(())
    })
    .delays(&mut budget)
    .sleep_with(RecordedDelays::default().sleep())
    .run();

    // Assert
    assert_that!(first).is_err();
    assert_that!(second).is_err();
    assert_that!(first_calls.get()).is_equal_to(5);
    assert_that!(second_calls.get()).is_equal_to(1);
}

#[test]
fn forever_retries_until_the_operation_succeeds() {
    // Arrange
    let calls = Calls::default();
    let operation = fails_until(&calls, 50);

    // Act
    let out = retry(operation)
        .delays(Fixed::of(Duration::ZERO).forever())
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Ok(50));
    assert_that!(calls.get()).is_equal_to(50);
}

#[test]
fn saturates_at_the_maximum_duration() {
    // Act
    let out: Result<(), RetryError<()>> = retry(|| Err(()))
        .delays([Duration::MAX; 2])
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out.map_err(|error| error.total_delay)).is_equal_to(Err(Duration::MAX));
}

#[test]
fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let delays = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<()>> = retry(|| Err(()))
        .delays(Fixed::of(1.seconds()).take(5))
        .delays([2.milliseconds(), 3.milliseconds()])
        .sleep_with(delays.sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: (),
        reason: Exhausted,
        tries: 3,
        total_delay: 5.milliseconds(),
    }));
    assert_that!(delays.into_inner()).contains_exactly([2.milliseconds(), 3.milliseconds()]);
}

#[test]
fn a_later_call_of_sleep_with_replaces_the_earlier_sleep() {
    // Arrange
    let earlier = RecordedDelays::default();
    let later = RecordedDelays::default();

    // Act
    let out: Result<(), RetryError<()>> = retry(|| Err(()))
        .sleep_with(earlier.sleep())
        .sleep_with(later.sleep())
        .delays([2.milliseconds(), 3.milliseconds()])
        .run();

    // Assert
    assert_that!(out).is_err();
    assert_that!(earlier.into_inner()).is_empty();
    assert_that!(later.into_inner()).contains_exactly([2.milliseconds(), 3.milliseconds()]);
}
