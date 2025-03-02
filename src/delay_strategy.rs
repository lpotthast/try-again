use crate::tracked_iterator::FiniteIterator;
use std::fmt::Debug;

/// We only implement `DelayStrategy` for any delay-yielding `FiniteIterator` by default.
/// A `FiniteIterator` is enforced, as we want users to always specify a concrete number of retries!
pub trait DelayStrategy<Delay>: Debug {
    fn next_delay(&mut self) -> Option<Delay>;
}

impl<Delay, I> DelayStrategy<Delay> for FiniteIterator<I>
where
    I: Iterator<Item = Delay> + Debug,
{
    fn next_delay(&mut self) -> Option<Delay> {
        self.next()
    }
}
