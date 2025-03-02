use crate::StdDuration;
use crate::tracked_iterator::{FiniteIterator, IntoTrackedIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct None;

impl None {
    pub fn take(self, count: usize) -> FiniteIterator<std::iter::Take<None>> {
        self.into_tracked().take(count)
    }
}

impl Iterator for None {
    type Item = StdDuration;

    fn next(&mut self) -> Option<Self::Item> {
        Some(StdDuration::ZERO)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use assertr::prelude::*;

    #[test]
    fn no_delay_strategy_always_returns_zero_duration() {
        let mut delay = None.take(3);

        assert_that(delay.next())
            .is_some()
            .is_equal_to(StdDuration::ZERO);
        assert_that(delay.next())
            .is_some()
            .is_equal_to(StdDuration::ZERO);
        assert_that(delay.next())
            .is_some()
            .is_equal_to(StdDuration::ZERO);
        assert_that(delay.next()).is_none();
    }
}
