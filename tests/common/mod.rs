//! Test helpers shared by the integration tests.

#![allow(dead_code, reason = "each test crate uses some of the helpers")]

use std::cell::{Cell, RefCell};
use std::future::{Ready, ready};
use std::rc::Rc;
use std::time::Duration;

/// Records the delays of a retry instead of waiting for them.
#[derive(Default)]
pub struct RecordedDelays(RefCell<Vec<Duration>>);

impl RecordedDelays {
    /// A sleep for a blocking retry that records each delay.
    pub fn sleep(&self) -> impl Fn(Duration) + Copy + '_ {
        |delay| self.0.borrow_mut().push(delay)
    }

    /// A sleep for an async retry that records each delay and completes at once. It fits only
    /// retries without time limits, whose waits it would complete at once too.
    pub fn async_sleep(&self) -> impl Fn(Duration) -> Ready<()> + '_ {
        completing_at_once(self.sleep())
    }

    pub fn into_inner(self) -> Vec<Duration> {
        self.0.into_inner()
    }
}

/// Counts the calls of an operation.
#[derive(Default)]
pub struct Calls(Cell<u32>);

impl Calls {
    /// Counts another call, and returns its number, starting at 1.
    pub fn next(&self) -> u32 {
        let call = self.0.get() + 1;
        self.0.set(call);
        call
    }

    /// The number of calls so far.
    pub fn get(&self) -> u32 {
        self.0.get()
    }
}

/// A failure that records in a shared flag whether it is still alive.
pub struct Held(Rc<Cell<bool>>);

impl Held {
    pub fn new(alive: &Rc<Cell<bool>>) -> Self {
        alive.set(true);
        Self(Rc::clone(alive))
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// Records whether the [`Held`] failure of the previous try is still alive during each delay.
pub struct FailureAliveDuringDelay {
    alive: Rc<Cell<bool>>,
    observed: RefCell<Vec<bool>>,
}

impl FailureAliveDuringDelay {
    pub fn new(alive: &Rc<Cell<bool>>) -> Self {
        Self {
            alive: Rc::clone(alive),
            observed: RefCell::new(Vec::new()),
        }
    }

    pub fn into_observed(self) -> Vec<bool> {
        self.observed.into_inner()
    }

    /// A sleep for a blocking retry that observes the failure during each delay.
    pub fn sleep(&self) -> impl Fn(Duration) + '_ {
        |_delay| self.observe()
    }

    /// A sleep for an async retry that observes the failure during each delay.
    pub fn async_sleep(&self) -> impl Fn(Duration) -> Ready<()> + '_ {
        completing_at_once(self.sleep())
    }

    fn observe(&self) {
        self.observed.borrow_mut().push(self.alive.get());
    }
}

/// The async form of a blocking `sleep`: a sleep that calls it, and completes at once.
fn completing_at_once(sleep: impl Fn(Duration)) -> impl Fn(Duration) -> Ready<()> {
    move |delay| {
        sleep(delay);
        ready(())
    }
}
