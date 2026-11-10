use super::*;

#[test]
fn accepts_a_closure_that_mutates_its_state() {
    // Arrange
    let mut calls = 0;

    // Act
    let out = retry(|| {
        calls += 1;
        if calls < 3 { Err(()) } else { Ok(calls) }
    })
    .delays(Fixed::of(Duration::ZERO).take(5))
    .sleep_with(RecordedDelays::default().sleep())
    .run();

    // Assert
    assert_that!(out).is_equal_to(Ok(3));
}

#[test]
fn never_retries_a_success_of_a_function() {
    // Arrange
    #[expect(
        clippy::unnecessary_wraps,
        reason = "an operation to retry must be fallible"
    )]
    fn operation() -> Result<u8, ()> {
        Ok(1)
    }
    let delays = RecordedDelays::default();

    // Act
    let out = retry(operation)
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(delays.sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Ok(1));
    assert_that!(delays.into_inner()).is_empty();
}

#[test]
fn retries_an_option_until_it_holds_a_value() {
    // Arrange
    let calls = Calls::default();
    let mut operation = fails_until(&calls, 2);

    // Act
    let out = retry(|| operation().ok())
        .delays([Duration::ZERO; 3])
        .sleep_with(RecordedDelays::default().sleep())
        .run();

    // Assert
    assert_that!(out).is_equal_to(Ok(2));
}

#[test]
fn the_error_of_an_option_passes_through_the_question_mark_with_its_chain() {
    // Arrange
    fn value() -> Result<u8, Box<dyn Error>> {
        let value = retry(|| None::<u8>)
            .delays([Duration::ZERO; 2])
            .sleep_with(RecordedDelays::default().sleep())
            .run()?;
        Ok(value)
    }

    // Act
    let error = value().expect_err("an operation without a value must fail");

    // Assert
    let chain: Vec<String> = std::iter::successors(Some(error.as_ref()), |&error| error.source())
        .map(ToString::to_string)
        .collect();
    assert_that!(chain).contains_exactly([
        "gave up after 3 tries: no retries left",
        "the operation returned no value",
    ]);
}
