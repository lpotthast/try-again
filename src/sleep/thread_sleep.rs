//! [`ThreadSleep`], which puts the thread to sleep.

use super::Sleep;
use core::time::Duration;

/// Puts the thread to sleep.
///
/// It is the default sleep of blocking retries. It is not available in the browser
/// (`wasm32-unknown-unknown`), which cannot block.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadSleep;

impl Sleep for ThreadSleep {
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
