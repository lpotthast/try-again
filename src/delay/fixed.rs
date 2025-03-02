use crate::StdDuration;
use crate::tracked_iterator::{FiniteIterator, IntoTrackedIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixed {
    pub delay: StdDuration,
}

impl Fixed {
    pub fn of(delay: impl Into<StdDuration>) -> Self {
        Self {
            delay: delay.into(),
        }
    }

    pub fn take(self, count: usize) -> FiniteIterator<std::iter::Take<Fixed>> {
        self.into_tracked().take(count)
    }
}

impl Iterator for Fixed {
    type Item = StdDuration;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.delay)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::IntoStdDuration;
    use assertr::prelude::*;

    #[test]
    fn static_delay_strategy_always_returns_the_configured_delay() {
        let mut delay = Fixed::of(50.millis()).take(3);

        assert_that(delay.next()).is_some().is_equal_to(50.millis());
        assert_that(delay.next()).is_some().is_equal_to(50.millis());
        assert_that(delay.next()).is_some().is_equal_to(50.millis());
        assert_that(delay.next()).is_none();
    }
}
