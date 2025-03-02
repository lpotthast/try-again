mod retry {
    use assertr::assert_that;
    use assertr::prelude::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI32, Ordering};
    use try_again::{IntoStdDuration, delay, retry};

    #[test]
    fn accepts_closure() {
        let test = || -> Result<(), ()> { Ok(()) };
        let out = retry(test).delayed_by(delay::None.take(0));
        assert_that(out).is_ok().is_equal_to(());
    }

    #[test]
    fn accepts_function_pointer() {
        fn test() -> Result<(), ()> {
            Ok(())
        }
        let out = retry(test).delayed_by(delay::None.take(3));
        assert_that(out).is_ok().is_equal_to(());
    }

    #[test]
    fn on_success_never_retries() {
        fn successful(counter: Arc<AtomicI32>) -> Result<i32, ()> {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = retry(|| successful(counter.clone())).delayed_by(delay::None.take(3));

        assert_that(out).is_ok().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_detail_message("Function must have been called 1 time only!")
            .is_equal_to(1);
    }

    #[test]
    fn on_continuous_error_retries_expected_number_of_times() {
        fn erroneous(counter: Arc<AtomicI32>) -> Result<(), i32> {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out =
            retry(|| erroneous(counter.clone())).delayed_by(delay::Fixed::of(50.millis()).take(3));

        assert_that(out).is_err().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_subject_name("Function")
            .is_equal_to(4);
    }
}

mod retry_with_options {
    use assertr::assert_that;
    use assertr::prelude::*;
    use std::marker::PhantomData;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI32, Ordering};
    use try_again::{
        IntoStdDuration, RetryOptions, delay, delay_executor::ThreadSleep, retry_with_options,
    };

    #[test]
    fn accepts_closure() {
        let test = || -> Result<(), ()> { Ok(()) };
        let out = retry_with_options(
            test,
            RetryOptions {
                delay_strategy: delay::None.take(0),
                delay_executor: ThreadSleep,
                _marker: PhantomData,
            },
        );
        assert_that(out).is_ok().is_equal_to(());
    }

    #[test]
    fn accepts_function_pointer() {
        fn test() -> Result<(), ()> {
            Ok(())
        }
        let out = retry_with_options(
            test,
            RetryOptions {
                delay_strategy: delay::None.take(0),
                delay_executor: ThreadSleep,
                _marker: PhantomData,
            },
        );
        assert_that(out).is_ok().is_equal_to(());
    }

    #[test]
    fn on_success_never_retries() {
        fn successful(counter: Arc<AtomicI32>) -> Result<i32, ()> {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            retry_with_options(
                || successful(counter.clone()),
                RetryOptions {
                    delay_strategy: delay::None.take(3),
                    delay_executor: ThreadSleep,
                    _marker: PhantomData,
                },
            )
        };

        assert_that(out).is_ok().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_detail_message("Function must have been called 1 time only!")
            .is_equal_to(1);
    }

    #[test]
    fn on_continuous_error_retries_expected_number_of_times() {
        fn erroneous(counter: Arc<AtomicI32>) -> Result<(), i32> {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(42)
        }

        let counter = Arc::new(AtomicI32::new(0));

        let out = {
            retry_with_options(
                || erroneous(counter.clone()),
                RetryOptions {
                    delay_strategy: delay::Fixed::of(50.millis()).take(3),
                    delay_executor: ThreadSleep,
                    _marker: PhantomData,
                },
            )
        };

        assert_that(out).is_err().is_equal_to(42);
        assert_that(counter.load(Ordering::SeqCst))
            .with_subject_name("Function")
            .is_equal_to(4);
    }
}
