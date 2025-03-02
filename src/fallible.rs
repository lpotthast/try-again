pub trait NeedsRetry {
    fn needs_retry(&self) -> bool;
}

impl<T, E> NeedsRetry for Result<T, E> {
    fn needs_retry(&self) -> bool {
        self.is_err()
    }
}

impl<T> NeedsRetry for Option<T> {
    fn needs_retry(&self) -> bool {
        self.is_none()
    }
}

impl NeedsRetry for std::process::ExitStatus {
    fn needs_retry(&self) -> bool {
        !self.success()
    }
}

#[cfg(test)]
mod test {
    use assertr::prelude::*;
    use super::NeedsRetry;

    #[test]
    fn result_does_not_need_retry_when_ok() {
        let result: Result<(), ()> = Ok(());
        assert_that(result.needs_retry()).is_false();
    }

    #[test]
    fn result_needs_retry_when_err() {
        let result: Result<(), ()> = Err(());
        assert_that(result.needs_retry()).is_true();
    }

    #[test]
    fn option_does_not_need_retry_when_some() {
        let option: Option<()> = Some(());
        assert_that(option.needs_retry()).is_false();
    }

    #[test]
    fn option_needs_retry_when_none() {
        let option: Option<()> = None;
        assert_that(option.needs_retry()).is_true();
    }

    #[test]
    fn exit_status_does_not_need_retry_when_successful() {
        let successful_exit_status = std::process::ExitStatus::default();
        assert_that(successful_exit_status.needs_retry()).is_false();
    }
}