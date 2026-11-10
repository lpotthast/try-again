use super::*;

#[tokio::test(start_paused = true)]
async fn retries_a_try_that_times_out() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<u32, RetryError<TryFailure<()>>> = retry_async(async || {
        let call = calls.next();
        // The first try hangs.
        if call == 1 {
            tokio::time::sleep(60.seconds()).await;
        }
        Ok(call)
    })
    .timeout_each(2.seconds())
    .delays(Fixed::of(1.seconds()).take(3))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Ok(2));
    assert_that!(start.elapsed()).is_equal_to(3.seconds());
}

#[tokio::test(start_paused = true)]
async fn when_may_stop_at_a_timeout() {
    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(60.seconds()).await;
        Err(())
    })
    .timeout_each(2.seconds())
    .when(|failure| !matches!(failure, TryFailure::TimedOut))
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::TimedOut,
        reason: Rejected,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
}

#[tokio::test(start_paused = true)]
async fn the_deadline_keeps_the_failure_before_a_try_that_it_cancels() {
    // Arrange
    let calls = Calls::default();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        if call > 1 {
            tokio::time::sleep(60.seconds()).await;
        }
        Err(call)
    })
    .timeout_each(10.seconds())
    .timeout_total(7.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The first try fails at once. The second starts at 1 second, and the deadline cancels it
    // at 7 seconds, before its timeout at 11 seconds.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(1),
        reason: Expired,
        tries: 2,
        total_delay: 1.seconds(),
    }));
}

#[tokio::test(start_paused = true)]
async fn a_try_that_times_out_as_the_deadline_expires_ends_the_retry() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(60.seconds()).await;
        Err(())
    })
    .timeout_each(10.seconds())
    .timeout_total(30.seconds())
    .delays(Fixed::of(Duration::ZERO).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The third try times out at 30 seconds, as the deadline expires.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::TimedOut,
        reason: Expired,
        tries: 3,
        total_delay: Duration::ZERO,
    }));
    assert_that!(start.elapsed()).is_equal_to(30.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_try_that_completes_as_it_times_out_counts() {
    // Act
    let at_once: Result<u8, RetryError<TryFailure<()>>> = retry_async(async || Ok(1))
        .timeout_each(Duration::ZERO)
        .delays([])
        .sleep_with(tokio_clock)
        .run()
        .await;
    let at_the_timeout: Result<u8, RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(2.seconds()).await;
        Ok(2)
    })
    .timeout_each(2.seconds())
    .delays([])
    .sleep_with(tokio_clock)
    .run()
    .await;
    let after_a_zero_timeout: Result<(), RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(1.seconds()).await;
        Ok(())
    })
    .timeout_each(Duration::ZERO)
    .delays([])
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(at_once).is_equal_to(Ok(1));
    assert_that!(at_the_timeout).is_equal_to(Ok(2));
    assert_that!(after_a_zero_timeout).is_equal_to(Err(RetryError {
        failure: TryFailure::TimedOut,
        reason: Exhausted,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
}

#[tokio::test(start_paused = true)]
async fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<u8, RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(2.seconds()).await;
        Ok(1)
    })
    .timeout_each(1.seconds())
    .timeout_each(5.seconds())
    .delays([])
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The earlier timeout would cancel the try after 1 second.
    assert_that!(out).is_equal_to(Ok(1));
    assert_that!(start.elapsed()).is_equal_to(2.seconds());
}

#[tokio::test(start_paused = true)]
async fn the_deadline_limits_the_whole_retry_in_either_order() {
    // Arrange
    let hangs = async || {
        tokio::time::sleep(60.seconds()).await;
        Err::<(), ()>(())
    };
    let start = Instant::now();

    // Act
    let before = retry_async(hangs)
        .timeout_each(4.seconds())
        .timeout_total(10.seconds())
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;
    let elapsed = start.elapsed();
    let after = retry_async(hangs)
        .timeout_total(10.seconds())
        .timeout_each(4.seconds())
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    // The tries start at 0 and 5 seconds and time out at 4 and 9 seconds. The delay after the
    // second ends at the deadline.
    assert_that!(before).is_equal_to(Err(RetryError {
        failure: TryFailure::TimedOut,
        reason: Expired,
        tries: 2,
        total_delay: 2.seconds(),
    }));
    assert_that!(elapsed).is_equal_to(10.seconds());
    assert_that!(after).is_equal_to(before);
}

#[tokio::test(start_paused = true)]
async fn retries_an_option_that_times_out() {
    // Arrange
    let calls = Calls::default();

    // Act
    let out: Result<u32, RetryError<TryFailure<NoValue>>> = retry_async(|| {
        let call = calls.next();
        async move {
            if call == 1 {
                std::future::pending::<()>().await;
            }
            (call == 3).then_some(call)
        }
    })
    .timeout_each(1.seconds())
    .delays(Fixed::of(1.seconds()).take(3))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The first try times out, the second returns `None`, and the third a value.
    assert_that!(out).is_equal_to(Ok(3));
}

#[tokio::test(start_paused = true)]
async fn the_closures_tell_a_timeout_from_a_failure() {
    // Arrange
    let calls = Calls::default();
    let seen = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<TryFailure<&str>>> = retry_async(async || {
        if calls.next() == 1 {
            tokio::time::sleep(1.minutes()).await;
        }
        Err("refused")
    })
    .timeout_each(2.seconds())
    .on_retry(|failure, _delay, tries| seen.borrow_mut().push((*failure, tries)))
    .delays(Fixed::of(1.seconds()).take(2))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(seen.into_inner()).is_equal_to(vec![
        (TryFailure::TimedOut, 1),
        (TryFailure::Failed("refused"), 2),
    ]);
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed("refused"),
        reason: Exhausted,
        tries: 3,
        total_delay: 2.seconds(),
    }));
}
