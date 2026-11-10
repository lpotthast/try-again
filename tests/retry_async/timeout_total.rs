use super::*;

#[tokio::test(start_paused = true)]
async fn returns_the_value_of_the_first_successful_try() {
    // Arrange
    let calls = Calls::default();

    // Act
    let out: Result<u32, RetryError<TryFailure<()>>> = retry_async(async || {
        let call = calls.next();
        if call < 3 { Err(()) } else { Ok(call) }
    })
    .timeout_total(10.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Ok(3));
}

#[tokio::test(start_paused = true)]
async fn gives_up_at_once_before_a_delay_that_would_pass_the_deadline() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || Err(calls.next()))
        .timeout_total(10.seconds())
        .delays(Exponential::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    // The tries run at 0, 1, 3 and 7 seconds. The delay of 8 seconds after the fourth would
    // pass the deadline, so the retry gives up without waiting for it.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(4),
        reason: Expired,
        tries: 4,
        total_delay: 7.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(7.seconds());
}

#[tokio::test(start_paused = true)]
async fn gives_up_when_the_deadline_expires_during_a_delay() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        tokio::time::sleep(1.seconds()).await;
        Err(call)
    })
    .timeout_total(9.seconds())
    .delays(Exponential::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The tries run from 0, 2 and 5 seconds, and take a second each, which the retry cannot
    // count. So it starts the delay of 4 seconds after the third try, at 6 seconds, and the
    // deadline cuts it short.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(3),
        reason: Expired,
        tries: 3,
        total_delay: 3.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(9.seconds());
}

#[tokio::test(start_paused = true)]
async fn cancels_a_try_that_runs_at_the_deadline_and_keeps_the_earlier_failure() {
    // Arrange
    let calls = Calls::default();
    let finished = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        tokio::time::sleep(4.seconds()).await;
        finished.next();
        Err(call)
    })
    .timeout_total(12.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The tries start at 0, 5 and 10 seconds and fail after 4 seconds each. The deadline
    // cancels the third at 12 seconds, so the failure of the second is the last.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(2),
        reason: Expired,
        tries: 3,
        total_delay: 2.seconds(),
    }));
    assert_that!(calls.get()).is_equal_to(3);
    assert_that!(finished.get()).is_equal_to(2);
    assert_that!(start.elapsed()).is_equal_to(12.seconds());
}

#[tokio::test(start_paused = true)]
async fn cancels_a_first_try_that_outlasts_the_deadline() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || {
        tokio::time::sleep(60.seconds()).await;
        Err(())
    })
    .timeout_total(3.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::TimedOut,
        reason: Expired,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
    assert_that!(start.elapsed()).is_equal_to(3.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_try_that_fails_as_the_deadline_expires_ends_the_retry_without_taking_a_delay() {
    // Arrange
    let calls = Calls::default();
    let mut delays = Fixed::of(1.seconds()).take(3);
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        tokio::time::sleep(5.seconds()).await;
        Err(call)
    })
    .timeout_total(5.seconds())
    .delays(&mut delays)
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // No retry follows, so the delays are left for the next retry that shares them.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(1),
        reason: Expired,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
    assert_that!(calls.get()).is_equal_to(1);
    assert_that!(start.elapsed()).is_equal_to(5.seconds());
    assert_that!(delays.count()).is_equal_to(3);
}

#[tokio::test(start_paused = true)]
async fn a_delay_that_ends_as_the_deadline_expires_ends_the_retry() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        tokio::time::sleep(500.milliseconds()).await;
        Err(call)
    })
    .timeout_total(3.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The tries run from 0 and 1.5 seconds, and take half a second each. The delay after the
    // second ends at the deadline, and counts, as the retry waited it in full.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(2),
        reason: Expired,
        tries: 2,
        total_delay: 2.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(3.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_zero_deadline_still_runs_the_first_try() {
    // Act
    let succeeded: Result<u8, RetryError<TryFailure<()>>> = retry_async(async || Ok(1))
        .timeout_total(Duration::ZERO)
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;
    let failed: Result<(), RetryError<TryFailure<u8>>> = retry_async(async || Err(1))
        .timeout_total(Duration::ZERO)
        .delays(Fixed::of(1.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(succeeded).is_equal_to(Ok(1));
    assert_that!(failed).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(1),
        reason: Expired,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
}

#[tokio::test(start_paused = true)]
async fn a_later_call_replaces_the_earlier_one() {
    // Arrange
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .timeout_total(1.seconds())
        .timeout_total(10.seconds())
        .delays(Fixed::of(4.seconds()).forever())
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    // The earlier deadline would leave no time for the first delay. With the later one, only a
    // third delay would end after the deadline.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(()),
        reason: Expired,
        tries: 3,
        total_delay: 8.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(8.seconds());
}

#[tokio::test(start_paused = true)]
async fn the_deadline_starts_at_the_first_try() {
    // Arrange
    let retry = retry_async(async || {
        tokio::time::sleep(1.seconds()).await;
        Err::<(), ()>(())
    })
    .timeout_total(5.seconds())
    .delays(Fixed::of(1.seconds()).forever());
    tokio::time::advance(60.seconds()).await;
    let start = Instant::now();

    // Act
    let out = retry.sleep_with(tokio_clock).run().await;

    // Assert
    assert_that!(out).is_err();
    assert_that!(start.elapsed()).is_equal_to(5.seconds());
}

#[tokio::test(start_paused = true)]
async fn the_deadline_runs_during_the_first_poll_of_the_first_try() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<u32, RetryError<TryFailure<u32>>> = retry_async(async || {
        let call = calls.next();
        if call > 1 {
            return Ok(call);
        }
        // The first try takes 8 seconds before it first yields.
        tokio::time::advance(8.seconds()).await;
        Err(1)
    })
    .timeout_total(10.seconds())
    .delays(Fixed::of(4.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    // The first try ends at 8 seconds, and the delay after it passes the deadline at 10
    // seconds.
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(1),
        reason: Expired,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
    assert_that!(start.elapsed()).is_equal_to(10.seconds());
}

#[tokio::test(start_paused = true)]
async fn ends_with_the_delays_before_the_deadline() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(async || Err(calls.next()))
        .timeout_total(30.seconds())
        .delays(Fixed::of(1.seconds()).take(2))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(3),
        reason: Exhausted,
        tries: 3,
        total_delay: 2.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(2.seconds());
}

#[tokio::test(start_paused = true)]
async fn a_failure_not_worth_another_try_ends_the_retry_before_the_deadline() {
    // Arrange
    let calls = Calls::default();
    let start = Instant::now();

    // Act
    let out: Result<(), RetryError<TryFailure<u16>>> = retry_async(async || {
        let call = calls.next();
        Err(if call < 3 { 503 } else { 401 })
    })
    .when(|status| *status >= 500)
    .timeout_total(30.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(401),
        reason: Rejected,
        tries: 3,
        total_delay: 2.seconds(),
    }));
    assert_that!(start.elapsed()).is_equal_to(2.seconds());
}

#[tokio::test(start_paused = true)]
async fn the_sleep_measures_the_deadline() {
    // Arrange
    let waits = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .timeout_total(10.seconds())
        .delays(Fixed::of(4.seconds()).forever())
        .sleep_with(|wait| {
            waits.borrow_mut().push(wait);
            tokio::time::sleep(wait)
        })
        .run()
        .await;

    // Assert
    // The deadline's wait starts first, and the delays run beside it. A third delay would end
    // after the deadline.
    assert_that!(out).is_err();
    assert_that!(waits.into_inner()).contains_exactly([10.seconds(), 4.seconds(), 4.seconds()]);
}

#[tokio::test(start_paused = true)]
async fn the_predicate_sees_a_failure_that_comes_as_the_deadline_expires() {
    // Act
    let out: Result<(), RetryError<TryFailure<u16>>> = retry_async(async || {
        tokio::time::sleep(5.seconds()).await;
        Err(401)
    })
    .when(|status| *status >= 500)
    .timeout_total(5.seconds())
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(401),
        reason: Rejected,
        tries: 1,
        total_delay: Duration::ZERO,
    }));
}

#[tokio::test(start_paused = true)]
async fn the_timers_start_before_the_closure_that_creates_the_first_try() {
    // Arrange
    let log = RefCell::new(Vec::new());

    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(|| {
        log.borrow_mut().push("try");
        async { Ok(()) }
    })
    .timeout_each(1.seconds())
    .timeout_total(5.seconds())
    .delays([])
    .sleep_with(|wait| {
        log.borrow_mut().push("timer");
        tokio::time::sleep(wait)
    })
    .run()
    .await;

    // Assert
    // The deadline's timer, then the timeout's timer, then the closure of the try.
    assert_that!(out).is_ok();
    assert_that!(log.into_inner()).contains_exactly(["timer", "timer", "try"]);
}

#[tokio::test]
async fn a_sleep_that_never_waits_cannot_outlast_the_deadline() {
    // Act
    let out: Result<(), RetryError<TryFailure<()>>> = retry_async(async || Err(()))
        .timeout_total(20.milliseconds())
        // Bounded, so that the test fails rather than hangs if the retry never yields.
        .delays(Fixed::of(Duration::ZERO).take(1_000_000))
        .sleep_with(eager_zero)
        .run()
        .await;

    // Assert
    // The retry yields once per retry, so the deadline's timer fires although no try or
    // delay waits.
    assert_that!(out.map_err(|error| error.reason)).is_equal_to(Err(Expired));
}

#[tokio::test]
async fn no_try_starts_after_the_deadline_expired() {
    // Arrange
    /// Lets the deadline pass right after the first delay ends, as when a busy executor runs the
    /// retry late.
    struct LateClock(Rc<Cell<bool>>);

    impl AsyncSleep for LateClock {
        type Future = Pin<Box<dyn Future<Output = ()>>>;

        fn sleep(&self, duration: Duration) -> Self::Future {
            let passed = Rc::clone(&self.0);
            if duration == DEADLINE {
                Box::pin(std::future::poll_fn(move |cx| {
                    if passed.get() {
                        std::task::Poll::Ready(())
                    } else {
                        cx.waker().wake_by_ref();
                        std::task::Poll::Pending
                    }
                }))
            } else {
                // The delay ends, and the deadline passes as it does.
                Box::pin(std::future::poll_fn(move |_cx| {
                    passed.set(true);
                    std::task::Poll::Ready(())
                }))
            }
        }
    }

    const DEADLINE: Duration = Duration::from_secs(10);
    let calls = Calls::default();

    // Act
    let out: Result<(), RetryError<TryFailure<u32>>> = retry_async(|| {
        let call = calls.next();
        async move { Err(call) }
    })
    .timeout_total(DEADLINE)
    .delays(Fixed::of(1.seconds()).forever())
    .sleep_with(LateClock(Rc::default()))
    .run()
    .await;

    // Assert
    assert_that!(calls.get()).is_equal_to(1);
    assert_that!(out).is_equal_to(Err(RetryError {
        failure: TryFailure::Failed(1),
        reason: Expired,
        tries: 1,
        total_delay: 1.seconds(),
    }));
}
