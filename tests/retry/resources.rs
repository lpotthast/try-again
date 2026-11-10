use super::*;

#[test]
fn a_failure_is_dropped_before_the_delay() {
    // Arrange
    let alive = Rc::new(Cell::new(false));
    let sleep = FailureAliveDuringDelay::new(&alive);

    // Act
    let out = retry(|| Err::<(), _>(Held::new(&alive)))
        .delays([Duration::ZERO; 2])
        .sleep_with(sleep.sleep())
        .run();

    // Assert
    assert_that!(out).is_err();
    assert_that!(sleep.into_observed()).contains_exactly([false, false]);
}
