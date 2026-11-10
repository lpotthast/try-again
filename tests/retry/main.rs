//! Tests of blocking retries.

use assertr::prelude::*;
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::rc::Rc;
use std::time::Duration;
use try_again::StopReason::{Exhausted, Rejected};
use try_again::delay::{DelaysExt, Exponential, Fixed};
use try_again::{DurationExt, RetryConfig, RetryError, retry};

#[path = "../common/mod.rs"]
mod common;
use common::{Calls, FailureAliveDuringDelay, Held, RecordedDelays};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    Transient,
    Permanent,
}

/// An operation that fails until its `succeed_on`-th call, counted in `calls`, and returns the
/// number of its call.
fn fails_until(calls: &Calls, succeed_on: u32) -> impl FnMut() -> Result<u32, u32> + '_ {
    move || {
        let call = calls.next();
        if call < succeed_on {
            Err(call)
        } else {
            Ok(call)
        }
    }
}

mod config;
mod delays;
mod on_retry;
mod operations;
mod override_next_delay;
mod resources;
#[cfg(feature = "std")]
mod sleeps;
mod when;
