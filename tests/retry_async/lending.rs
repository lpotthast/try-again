//! `retry_async_lending` needs `std`.

use super::*;
use try_again::retry_async_lending;

/// A connection whose queries need `&mut self`. The first `failures` queries fail, and the
/// query with the number `hangs_at`, if any, never completes.
struct Connection {
    queries: u32,
    failures: u32,
    hangs_at: Option<u32>,
}

impl Connection {
    fn new(failures: u32, hangs_at: Option<u32>) -> Self {
        Self {
            queries: 0,
            failures,
            hangs_at,
        }
    }

    async fn query(&mut self) -> Result<u32, u32> {
        self.queries += 1;
        if self.hangs_at == Some(self.queries) {
            std::future::pending::<()>().await;
        }
        if self.queries > self.failures {
            Ok(self.queries)
        } else {
            Err(self.queries)
        }
    }
}

#[tokio::test(start_paused = true)]
async fn lends_the_state_to_each_try_and_returns_the_borrow() {
    // Arrange
    let mut connection = Connection::new(2, None);

    // Act
    let out = retry_async_lending(&mut connection, |mut connection| async move {
        connection.query().await
    })
    .delays(Fixed::of(1.seconds()).take(3))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Ok(3));
    assert_that!(connection.queries).is_equal_to(3);
}

#[tokio::test(start_paused = true)]
async fn a_try_that_times_out_returns_the_state_to_the_next_try() {
    // Arrange
    let mut connection = Connection::new(2, Some(2));

    // Act
    let out = retry_async_lending(&mut connection, |mut connection| async move {
        connection.query().await
    })
    .timeout_each(5.seconds())
    .delays(Fixed::of(1.seconds()).take(3))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The second query hangs until its timeout cancels it. The third succeeds.
    assert_that!(out).is_equal_to(Ok(3));
    assert_that!(connection.queries).is_equal_to(3);
}

#[tokio::test(start_paused = true)]
async fn a_try_that_the_deadline_cancels_returns_the_borrow() {
    // Arrange
    let mut connection = Connection::new(u32::MAX, Some(2));

    // Act
    let out = retry_async_lending(&mut connection, |mut connection| async move {
        connection.query().await
    })
    .timeout_total(10.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(1),
        reason: Expired,
        tries: 2,
        total_delay: 1.seconds(),
    }));
    connection.hangs_at = None;
    assert_that!(connection.query().await).is_equal_to(Err(3));
}

#[tokio::test(start_paused = true)]
async fn a_lending_retry_can_be_spawned() {
    // Arrange
    let task = tokio::spawn(async {
        let mut connection = Connection::new(2, Some(2));
        retry_async_lending(&mut connection, |mut connection| async move {
            connection.query().await
        })
        .timeout_each(5.seconds())
        .timeout_total(30.seconds())
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await
    });

    // Act
    let out = task.await.expect("the task must not panic");

    // Assert
    assert_that!(out).is_equal_to(Ok(3));
}

#[tokio::test(start_paused = true)]
async fn lends_unsized_state() {
    // Arrange
    let mut counts = [0_u32; 3];

    // Act
    let out: Result<(), RetryError<()>> =
        retry_async_lending(counts.as_mut_slice(), |mut counts| async move {
            counts[0] += 1;
            Err(())
        })
        .delays(Fixed::of(1.seconds()).take(2))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out.map_err(|error| error.tries)).is_equal_to(Err(3));
    assert_that!(counts).is_equal_to([3, 0, 0]);
}

#[tokio::test(start_paused = true)]
#[should_panic(expected = "still lent to an earlier try")]
async fn a_leaked_loan_panics_the_next_try() {
    let mut connection = Connection::new(u32::MAX, None);

    let _ = retry_async_lending(&mut connection, |connection| {
        std::mem::forget(connection);
        async { Err::<(), ()>(()) }
    })
    .delays(Fixed::of(1.seconds()).take(1))
    .sleep_with(tokio_clock)
    .run()
    .await;
}

#[tokio::test(start_paused = true)]
#[should_panic(expected = "part of a failure that a retry with a deadline keeps")]
async fn with_a_deadline_a_failure_that_holds_the_loan_panics_the_next_try() {
    let mut count = 0_u32;

    let _ = retry_async_lending(&mut count, |mut count| async move {
        *count += 1;
        Err::<(), _>(count)
    })
    .timeout_total(10.seconds())
    .delays(Fixed::of(1.seconds()).take(1))
    .sleep_with(tokio_clock)
    .run()
    .await;
}
