//! The settings of a retry together: their order, and the settings that retries share, a
//! `RetryConfig` applied with `Retry::config`.

use super::*;
use std::iter::Take;
use try_again::delay::FixedDelays;

#[test]
fn each_retry_gets_its_own_delays_from_a_shared_config() {
    // Arrange
    let config = RetryConfig::new().delays(Fixed::of(1.milliseconds()).take(2));
    let delays = RecordedDelays::default();

    // Act
    let first = retry(|| Err::<(), _>("down"))
        .config(&config)
        .sleep_with(delays.sleep())
        .run();
    let second = retry(|| Err::<(), _>("down"))
        .config(&config)
        .sleep_with(delays.sleep())
        .run();

    // Assert
    assert_that!(first.map_err(|error| error.tries)).is_equal_to(Err(3));
    assert_that!(second.map_err(|error| error.tries)).is_equal_to(Err(3));
    assert_that!(delays.into_inner()).is_equal_to(vec![1.milliseconds(); 4]);
}

#[test]
fn a_shared_config_brings_its_sleep() {
    // Arrange
    let delays = RecordedDelays::default();
    let config = RetryConfig::new()
        .delays(Fixed::of(1.milliseconds()).take(2))
        .sleep_with(delays.sleep());

    // Act
    let out = retry(|| Err::<(), _>("down")).config(&config).run();

    // Assert
    assert_that!(out.map_err(|error| error.tries)).is_equal_to(Err(3));
    assert_that!(delays.into_inner()).is_equal_to(vec![1.milliseconds(); 2]);
}

#[test]
fn settings_that_the_config_does_not_make_come_after_it() {
    // Arrange
    let config = RetryConfig::new().when(|failure: &u32| *failure < 3);
    let calls = Calls::default();
    let seen = RefCell::new(Vec::new());

    // Act
    let out = retry(fails_until(&calls, u32::MAX))
        .config(&config)
        .on_retry(|_failure, _delay, tries| seen.borrow_mut().push(tries))
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
    assert_that!(seen.into_inner()).is_equal_to(vec![1, 2]);
}

#[test]
fn a_struct_keeps_a_config_with_function_pointers() {
    // Arrange
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "a predicate receives the failure by reference"
    )]
    fn is_transient(failure: &Failure) -> bool {
        *failure == Failure::Transient
    }

    type Retries = RetryConfig<Take<FixedDelays>, (), (), fn(&Failure) -> bool>;

    struct Client {
        retries: Retries,
    }

    let client = Client {
        retries: RetryConfig::new()
            .when(is_transient as fn(&Failure) -> bool)
            .delays(Fixed::of(1.milliseconds()).take(5)),
    };
    let calls = Calls::default();

    // Act
    let out: Result<(), RetryError<Failure>> = retry(|| {
        let call = calls.next();
        Err(if call < 3 {
            Failure::Transient
        } else {
            Failure::Permanent
        })
    })
    .config(&client.retries)
    .sleep_with(RecordedDelays::default().sleep())
    .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: Failure::Permanent,
        reason: Rejected,
        tries: 3,
        total_delay: 2.milliseconds(),
    }));
}

#[test]
fn the_closures_of_a_shared_config_infer_their_failure_from_the_retry() {
    // Arrange
    let seen = RefCell::new(Vec::new());
    let config = RetryConfig::new()
        .on_retry(|_failure, delay, tries| seen.borrow_mut().push((delay, tries)))
        .delays(Fixed::of(1.milliseconds()).take(2));

    // Act
    let out = retry(|| Err::<(), _>("down"))
        .config(&config)
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out.map_err(|error| error.tries)).is_equal_to(Err(3));
    assert_that!(seen.into_inner())
        .contains_exactly([(1.milliseconds(), 1), (1.milliseconds(), 2)]);
}

/// What the hook and the sleep of [`shared`] received.
#[derive(Default)]
struct Recorded {
    /// The delay and the number of tries of each call of the hook.
    seen: RefCell<Vec<(Duration, u32)>>,
    slept: RecordedDelays,
}

/// A config that makes every setting of a blocking retry: it retries the failures below 3, takes
/// up to 5 delays of 1 ms, doubles each, and records what its hook and its sleep receive.
#[expect(
    clippy::type_complexity,
    reason = "the type of a config records all of its settings"
)]
fn shared(
    recorded: &Recorded,
) -> RetryConfig<
    Take<FixedDelays>,
    (),
    (),
    impl FnMut(&u32) -> bool + Clone,
    impl FnMut(&u32, Duration) -> Duration + Clone,
    impl FnMut(&u32, Duration, u32) + Clone,
    impl Fn(Duration) + Copy,
> {
    RetryConfig::new()
        .when(|failure: &u32| *failure < 3)
        .override_next_delay(|_failure: &u32, delay| delay * 2)
        .on_retry(|_failure: &u32, delay, tries| recorded.seen.borrow_mut().push((delay, tries)))
        .delays(Fixed::of(1.milliseconds()).take(5))
        .sleep_with(recorded.slept.sleep())
}

#[test]
fn delays_after_a_shared_config_replace_its_delays() {
    // Arrange
    let recorded = Recorded::default();
    let calls = Calls::default();

    // Act
    let out = retry(fails_until(&calls, u32::MAX))
        .config(&shared(&recorded))
        .delays(Fixed::of(5.milliseconds()).take(5))
        .run();

    // Assert
    // The other settings of the config still apply: its predicate rejects the third failure, its
    // override doubles each delay, and its hook and its sleep record.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: 20.milliseconds(),
    }));
    assert_that!(recorded.seen.into_inner())
        .contains_exactly([(10.milliseconds(), 1), (10.milliseconds(), 2)]);
    assert_that!(recorded.slept.into_inner()).contains_exactly([10.milliseconds(); 2]);
}

#[test]
fn a_predicate_after_a_shared_config_replaces_its_predicate() {
    // Arrange
    let recorded = Recorded::default();
    let calls = Calls::default();

    // Act
    let out = retry(fails_until(&calls, u32::MAX))
        .config(&shared(&recorded))
        .when(|failure| *failure < 4)
        .run();

    // Assert
    // The predicate of the config would have rejected the third failure.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 4,
        reason: Rejected,
        tries: 4,
        total_delay: 6.milliseconds(),
    }));
    assert_that!(recorded.seen.into_inner()).contains_exactly([
        (2.milliseconds(), 1),
        (2.milliseconds(), 2),
        (2.milliseconds(), 3),
    ]);
    assert_that!(recorded.slept.into_inner()).contains_exactly([2.milliseconds(); 3]);
}

#[test]
fn an_override_after_a_shared_config_replaces_its_override() {
    // Arrange
    let recorded = Recorded::default();
    let calls = Calls::default();

    // Act
    let out = retry(fails_until(&calls, u32::MAX))
        .config(&shared(&recorded))
        .override_next_delay(|_failure, delay| delay * 3)
        .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: 6.milliseconds(),
    }));
    assert_that!(recorded.seen.into_inner())
        .contains_exactly([(3.milliseconds(), 1), (3.milliseconds(), 2)]);
    assert_that!(recorded.slept.into_inner()).contains_exactly([3.milliseconds(); 2]);
}

#[test]
fn a_hook_after_a_shared_config_replaces_its_hook() {
    // Arrange
    let recorded = Recorded::default();
    let calls = Calls::default();
    let seen = RefCell::new(Vec::new());

    // Act
    let out = retry(fails_until(&calls, u32::MAX))
        .config(&shared(&recorded))
        .on_retry(|failure, _delay, _tries| seen.borrow_mut().push(*failure))
        .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: 4.milliseconds(),
    }));
    assert_that!(seen.into_inner()).contains_exactly([1, 2]);
    assert_that!(recorded.seen.into_inner()).is_empty();
    assert_that!(recorded.slept.into_inner()).contains_exactly([2.milliseconds(); 2]);
}

#[test]
fn a_sleep_after_a_shared_config_replaces_its_sleep() {
    // Arrange
    let recorded = Recorded::default();
    let calls = Calls::default();
    let slept = RecordedDelays::default();

    // Act
    let out = retry(fails_until(&calls, u32::MAX))
        .config(&shared(&recorded))
        .sleep_with(slept.sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: 4.milliseconds(),
    }));
    assert_that!(slept.into_inner()).contains_exactly([2.milliseconds(); 2]);
    assert_that!(recorded.slept.into_inner()).is_empty();
    assert_that!(recorded.seen.into_inner())
        .contains_exactly([(2.milliseconds(), 1), (2.milliseconds(), 2)]);
}

#[test]
fn the_builder_methods_can_come_in_any_order() {
    // Arrange
    let first_calls = Calls::default();
    let second_calls = Calls::default();
    let seen = RefCell::new(Vec::new());
    let slept = RecordedDelays::default();

    // Act
    let forward = retry(fails_until(&first_calls, u32::MAX))
        .delays(Fixed::of(1.milliseconds()).forever())
        .when(|failure| *failure < 3)
        .override_next_delay(|_failure, delay| delay * 2)
        .on_retry(|failure, delay, tries| seen.borrow_mut().push((*failure, delay, tries)))
        .sleep_with(slept.sleep())
        .run();
    let backward = retry(fails_until(&second_calls, u32::MAX))
        .sleep_with(slept.sleep())
        .on_retry(|failure, delay, tries| seen.borrow_mut().push((*failure, delay, tries)))
        .override_next_delay(|_failure, delay| delay * 2)
        .when(|failure| *failure < 3)
        .delays(Fixed::of(1.milliseconds()).forever())
        .run();

    // Assert
    assert_that!(forward).is_equal_to(Err(RetryError {
        failure: 3,
        reason: Rejected,
        tries: 3,
        total_delay: 4.milliseconds(),
    }));
    assert_that!(backward).is_equal_to(forward);
    // Each retry ran its hook and its sleep twice, with the overridden delay.
    assert_that!(seen.into_inner()).contains_exactly([
        (1, 2.milliseconds(), 1),
        (2, 2.milliseconds(), 2),
        (1, 2.milliseconds(), 1),
        (2, 2.milliseconds(), 2),
    ]);
    assert_that!(slept.into_inner()).contains_exactly([2.milliseconds(); 4]);
}

#[test]
fn shows_its_settings_in_debug() {
    let config = RetryConfig::new().delays([1.seconds()]);
    let retry = retry(|| Err::<(), ()>(())).config(&config);

    assert_that!(format!("{config:?}")).is_equal_to(
        "RetryConfig { delays: IntoIter([1s]), timeout_total: (), timeout_each: (), .. }",
    );
    assert_that!(format!("{retry:?}")).is_equal_to(
        "Retry { settings: RetryConfig { delays: IntoIter([1s]), timeout_total: (), \
         timeout_each: (), .. }, .. }",
    );
}
