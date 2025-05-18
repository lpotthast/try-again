mod retry_async {
    use assertr::assert_that;
    use assertr::prelude::*;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI32, Ordering};
    use try_again::{IntoStdDuration, delay, retry_async};

    #[tokio::test]
    async fn accepts_function_pointer() {
        async fn test() -> Result<(), ()> {
            Ok(())
        }
        let out = retry_async(test).delayed_by(delay::None.take(0)).await;
        assert_that(out).is_ok().is_equal_to(());
    }

    #[tokio::test]
    async fn accepts_closure() {
        let test = async || -> Result<(), ()> { Ok(()) };
        let out = retry_async(test).delayed_by(delay::None.take(0)).await;
        assert_that(out).is_ok().is_equal_to(());
    }

    #[tokio::test]
    async fn accepts_closure_capturing_owned_value() {
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct Data;

        let data = Data;
        let out: Result<Data, ()> = retry_async(move || {
            let data = data.clone();
            async move { Ok(data) }
        })
        .delayed_by(delay::None.take(0))
        .await;
        assert_that(out).is_ok().is_equal_to(Data);
    }

    #[tokio::test]
    async fn accepts_closure_capturing_reference() {
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct Data;

        async fn foo(_data: &Data) -> Result<(), ()> {
            Ok(())
        }

        let owned = Data;
        let reference = &owned;
        let out: Result<(), ()> = retry_async(|| foo(reference))
            .delayed_by(delay::None.take(0))
            .await;
        assert_that(out).is_ok().is_equal_to(());
    }

    #[tokio::test]
    async fn accepts_closure_capturing_non_send_data() {
        async fn foo(data: Rc<i32>) -> Result<i32, ()> {
            Ok(*data)
        }

        let data = Rc::new(0);
        let out: Result<i32, ()> = retry_async(|| foo(data.clone()))
            .delayed_by(delay::None.take(0))
            .await;
        assert_that(out).is_ok().is_equal_to(0);
    }

    #[tokio::test]
    async fn on_success_never_retries() {
        async fn successful(counter: Arc<AtomicI32>) -> Result<i32, ()> {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            retry_async(async || successful(counter.clone()).await)
                .delayed_by(delay::Fixed::of(50.millis()).take(3))
                .await
        };

        assert_that(out).is_ok().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_detail_message("Function must have been called 1 time only!")
            .is_equal_to(1);
    }

    #[tokio::test]
    async fn on_continuous_error_retries_expected_number_of_times() {
        async fn erroneous(counter: Arc<AtomicI32>) -> Result<(), i32> {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            let counter = counter.clone();
            retry_async(async || erroneous(counter.clone()).await)
                .delayed_by(delay::Fixed::of(50.millis()).take(3))
                .await
        };

        assert_that(out).is_err().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_subject_name("Function")
            .is_equal_to(4);
    }
}

mod retry_async_with_options {
    use assertr::assert_that;
    use assertr::prelude::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI32, Ordering};
    use try_again::{
        IntoStdDuration, RetryAsyncOptions, delay, delay_executor::TokioSleep,
        retry_async_with_options,
    };

    #[tokio::test]
    async fn accepts_closure() {
        let test = async || -> Result<(), ()> { Ok(()) };
        let out = retry_async_with_options(
            test,
            RetryAsyncOptions {
                delay_strategy: delay::None.take(0),
                delay_executor: TokioSleep,
                _marker: Default::default(),
            },
        )
        .await;
        assert_that(out).is_ok().is_equal_to(());
    }

    #[tokio::test]
    async fn accepts_function_pointer() {
        async fn test() -> Result<(), ()> {
            Ok(())
        }
        let out = retry_async_with_options(
            test,
            RetryAsyncOptions {
                delay_strategy: delay::None.take(0),
                delay_executor: TokioSleep,
                _marker: Default::default(),
            },
        )
        .await;
        assert_that(out).is_ok().is_equal_to(());
    }

    #[tokio::test]
    async fn on_success_never_retries() {
        async fn successful(counter: Arc<AtomicI32>) -> Result<i32, ()> {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            retry_async_with_options(
                async || successful(counter.clone()).await,
                RetryAsyncOptions {
                    delay_strategy: delay::Fixed::of(50.millis()).take(3),
                    delay_executor: TokioSleep,
                    _marker: Default::default(),
                },
            )
            .await
        };

        assert_that(out).is_ok().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_detail_message("Function must have been called 1 time only!")
            .is_equal_to(1);
    }

    #[tokio::test]
    async fn on_continuous_error_retries_expected_number_of_times() {
        async fn erroneous(counter: Arc<AtomicI32>) -> Result<(), i32> {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            let counter = counter.clone();
            retry_async_with_options(
                async || erroneous(counter.clone()).await,
                RetryAsyncOptions {
                    delay_strategy: delay::Fixed::of(50.millis()).take(3),
                    delay_executor: TokioSleep,
                    _marker: Default::default(),
                },
            )
            .await
        };

        assert_that(out).is_err().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_subject_name("Function")
            .is_equal_to(4);
    }
}
