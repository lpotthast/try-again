//! `ThreadSleep`, the default sleep of a blocking retry, which needs `std`. Without it, a blocking
//! retry sleeps with the sleep that `sleep_with` hands it, as in the other modules.

use super::*;
use std::time::Instant;

#[test]
fn puts_the_thread_to_sleep_by_default() {
    // Arrange
    let calls = Calls::default();
    let operation = fails_until(&calls, 3);
    let start = Instant::now();

    // Act
    let out = retry(operation)
        .delays(Fixed::of(10.milliseconds()).take(5))
        .run();

    // Assert
    assert_that!(out).is_equal_to(Ok(3));
    assert_that!(start.elapsed()).is_greater_or_equal_to(20.milliseconds());
}
