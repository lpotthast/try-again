use crate::StdDuration;
use crate::tracked_iterator::{FiniteIterator, IntoTrackedIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExponentialBackoff {
    pub initial_delay: StdDuration,
}

impl ExponentialBackoff {
    pub fn of_initial_delay(initial_delay: impl Into<StdDuration>) -> Self {
        Self {
            initial_delay: initial_delay.into(),
        }
    }

    pub fn uncapped(self) -> ExponentialBackoffWithCap {
        ExponentialBackoffWithCap {
            initial_delay: self.initial_delay,
            last_delay: StdDuration::ZERO,
            max_delay: None,
            first: true,
        }
    }

    pub fn capped_at(self, max_delay: impl Into<StdDuration>) -> ExponentialBackoffWithCap {
        ExponentialBackoffWithCap {
            initial_delay: self.initial_delay,
            last_delay: StdDuration::ZERO,
            max_delay: Some(max_delay.into()),
            first: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExponentialBackoffWithCap {
    pub initial_delay: StdDuration,
    pub last_delay: StdDuration,
    pub max_delay: Option<StdDuration>,
    pub first: bool,
}

impl ExponentialBackoffWithCap {
    pub fn take(self, count: usize) -> FiniteIterator<std::iter::Take<ExponentialBackoffWithCap>> {
        self.into_tracked().take(count)
    }
}

impl Iterator for ExponentialBackoffWithCap {
    type Item = StdDuration;

    fn next(&mut self) -> Option<Self::Item> {
        if self.first {
            self.first = false;
            self.last_delay = self.initial_delay;
            return Some(self.initial_delay);
        }

        let mut next = self.last_delay * 2;
        if let Some(max_delay) = self.max_delay {
            if next > max_delay {
                next = max_delay;
            }
        }
        self.last_delay = next;
        Some(next)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::IntoStdDuration;
    use assertr::prelude::*;

    #[test]
    fn uncapped_exponential_backoff_delay_strategy_returns_initial_delay_for_the_first_try_and_doubles_the_delay_for_each_retry_until_reaching_max_tries()
     {
        let mut delay = ExponentialBackoff::of_initial_delay(50.millis())
            .uncapped()
            .take(4);

        assert_that(delay.next()).is_some().is_equal_to(50.millis());
        assert_that(delay.next())
            .is_some()
            .is_equal_to(100.millis());
        assert_that(delay.next())
            .is_some()
            .is_equal_to(200.millis());
        assert_that(delay.next())
            .is_some()
            .is_equal_to(400.millis());
        assert_that(delay.next()).is_none();
    }

    #[test]
    fn capped_exponential_backoff_delay_strategy_returns_initial_delay_for_the_first_try_and_doubles_the_delay_for_each_retry_until_capping_at_specified_max_delay_before_reaching_max_tries()
     {
        let mut delay = ExponentialBackoff::of_initial_delay(50.millis())
            .capped_at(250.millis())
            .take(5);

        assert_that(delay.next()).is_some().is_equal_to(50.millis());
        assert_that(delay.next())
            .is_some()
            .is_equal_to(100.millis());
        assert_that(delay.next())
            .is_some()
            .is_equal_to(200.millis());
        assert_that(delay.next())
            .is_some()
            .is_equal_to(250.millis());
        assert_that(delay.next())
            .is_some()
            .is_equal_to(250.millis());
        assert_that(delay.next()).is_none();
    }
}
