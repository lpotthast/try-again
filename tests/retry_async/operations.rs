use super::*;

#[tokio::test]
async fn accepts_a_function_pointer() {
    // Arrange
    async fn operation() -> Result<(), ()> {
        Ok(())
    }

    // Act
    let out = retry_async(operation)
        .delays(Fixed::of(Duration::ZERO).take(3))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out).is_ok();
}

#[tokio::test]
async fn accepts_a_closure_capturing_a_reference() {
    // Arrange
    #[derive(Debug)]
    struct Data;

    #[expect(clippy::unused_async, reason = "stands in for an async operation")]
    async fn use_data(_data: &Data) -> Result<(), ()> {
        Ok(())
    }

    let data = Data;

    // Act
    let out = retry_async(async || use_data(&data).await)
        .delays(Fixed::of(Duration::ZERO).take(3))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out).is_ok();
}

#[tokio::test]
async fn accepts_a_closure_capturing_data_that_is_not_send() {
    // Arrange
    let data = Rc::new(7);

    // Act
    let out: Result<i32, RetryError<()>> = retry_async(async || Ok(*data))
        .delays(Fixed::of(Duration::ZERO).take(3))
        .sleep_with(tokio_clock)
        .run()
        .await;

    // Assert
    assert_that!(out).is_equal_to(Ok(7));
}

#[tokio::test]
async fn accepts_a_closure_returning_an_async_block() {
    // Arrange
    let calls = Arc::new(AtomicU32::new(0));

    // Act
    let out: Result<u32, RetryError<()>> = retry_async(|| {
        let calls = Arc::clone(&calls);
        async move {
            let calls = calls.fetch_add(1, Ordering::SeqCst) + 1;
            if calls < 3 { Err(()) } else { Ok(calls) }
        }
    })
    .delays(Fixed::of(1.milliseconds()).take(5))
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Ok(3));
}

#[tokio::test]
async fn retries_an_option_until_it_holds_a_value() {
    // Arrange
    let calls = Calls::default();

    // Act
    let out = retry_async(async || {
        let call = calls.next();
        (call == 2).then_some("value")
    })
    .delays([Duration::ZERO; 3])
    .sleep_with(tokio_clock)
    .run()
    .await;

    // Assert
    assert_that!(out).is_equal_to(Ok("value"));
    assert_that!(calls.get()).is_equal_to(2);
}
