//! The futures of retries must be `Send`, so that `tokio::spawn` accepts them.

use super::*;

fn assert_send<T: Send>(value: T) -> T {
    value
}

#[tokio::test(start_paused = true)]
async fn a_retry_with_a_closure_that_borrows_can_be_spawned() {
    // Arrange
    let task = tokio::spawn(async {
        let tries = AtomicU32::new(0);
        retry_async(async || connect(&tries).await)
            .when(|error| error.kind() == std::io::ErrorKind::Other)
            .delays(Exponential::of(1.seconds()).capped_at(5.seconds()).take(5))
            .sleep_with(tokio_clock)
            .run()
            .await
    });

    // Act
    let out = task.await.expect("the task must not panic");

    // Assert
    assert_that!(out).ok().is_equal_to(3);
}

#[tokio::test(start_paused = true)]
async fn a_retry_with_a_deadline_can_be_spawned() {
    // Arrange
    let task = tokio::spawn(async {
        let tries = AtomicU32::new(0);
        retry_async(async || connect(&tries).await)
            .timeout_total(30.seconds())
            .delays(Exponential::of(1.seconds()).forever())
            .sleep_with(tokio_clock)
            .run()
            .await
    });

    // Act
    let out = task.await.expect("the task must not panic");

    // Assert
    assert_that!(out).ok().is_equal_to(3);
}

#[tokio::test(start_paused = true)]
async fn a_retry_with_a_timeout_for_each_try_can_be_spawned() {
    // Arrange
    let task = tokio::spawn(async {
        let tries = AtomicU32::new(0);
        retry_async(async || connect(&tries).await)
            .timeout_each(5.seconds())
            .on_retry(|_, _, _| {})
            .delays(Exponential::of(1.seconds()).take(5))
            .sleep_with(tokio_clock)
            .run()
            .await
    });

    // Act
    let out = task.await.expect("the task must not panic");

    // Assert
    assert_that!(out).ok().is_equal_to(3);
}

#[tokio::test(start_paused = true)]
async fn a_retry_whose_error_is_not_send_is_send() {
    // Arrange
    async fn operation() -> Result<(), Box<dyn Error>> {
        Err("not yet".into())
    }

    // Act
    let out = assert_send(
        retry_async(operation)
            .delays(Fixed::of(1.seconds()).take(2))
            .sleep_with(tokio_clock)
            .run(),
    )
    .await;

    // Assert
    assert_that!(out.map_err(|error| error.failure.to_string()))
        .is_equal_to(Err("not yet".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn a_retry_with_a_timeout_for_each_try_whose_error_is_not_send_is_send() {
    // Arrange
    async fn operation() -> Result<(), Box<dyn Error>> {
        Err("not yet".into())
    }

    // Act
    // Only without a deadline: a retry with one keeps the last failure while it waits, so its
    // future is `Send` only when the failure is.
    let out = assert_send(
        retry_async(operation)
            .timeout_each(5.seconds())
            .delays(Fixed::of(1.seconds()).take(2))
            .sleep_with(tokio_clock)
            .run(),
    )
    .await;

    // Assert
    assert_that!(out.map_err(|error| error.failure.to_string()))
        .is_equal_to(Err("not yet".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn a_retry_without_time_limits_is_send_with_a_sleep_that_is_not_sync() {
    // Arrange
    let waits = Calls::default();
    // The closure owns the counter, a `Cell`, so it is `Send`, but not `Sync`.
    let sleep = move |delay: Duration| {
        waits.next();
        tokio::time::sleep(delay)
    };

    // Act
    let out = assert_send(
        retry_async(async || Err::<(), ()>(()))
            .delays(Fixed::of(1.seconds()).take(2))
            .sleep_with(sleep)
            .run(),
    )
    .await;

    // Assert
    assert_that!(out.map_err(|error| error.tries)).is_equal_to(Err(3));
}

#[tokio::test(start_paused = true)]
async fn a_retry_with_a_deadline_is_send_even_when_its_success_is_not() {
    // Arrange
    async fn operation() -> Result<Rc<u8>, std::io::Error> {
        Ok(Rc::new(7))
    }

    // Act
    let deadline = assert_send(
        retry_async(operation)
            .timeout_total(5.seconds())
            .delays(Fixed::of(1.seconds()).forever())
            .sleep_with(tokio_clock)
            .run(),
    )
    .await;
    let both = assert_send(
        retry_async(operation)
            .timeout_each(1.seconds())
            .timeout_total(5.seconds())
            .delays(Fixed::of(1.seconds()).forever())
            .sleep_with(tokio_clock)
            .run(),
    )
    .await;

    // Assert
    assert_that!(deadline.ok().map(|value| *value)).is_equal_to(Some(7));
    assert_that!(both.ok().map(|value| *value)).is_equal_to(Some(7));
}
