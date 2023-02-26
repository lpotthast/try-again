use std::time::Duration;

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

pub trait RetryStrategy<D> {
    fn initial_delay(&self) -> D;
    fn next_delay(&self, tries: usize, last_delay: D) -> D;
    fn is_exhausted(&self, tries: usize) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Delay {
    Static {
        delay: Duration,
    },
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Option<Duration>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Retry {
    pub max_tries: usize,
    pub delay: Option<Delay>,
}

impl Retry {
    /// Retry for a certain time without any delay in between retries.
    /// This is a good fit when using the synchronous `retry` function, as any delay would block the current thread.
    pub fn max_tries(max_tries: usize) -> Self {
        Self {
            max_tries,
            delay: None,
        }
    }
}

impl RetryStrategy<Duration> for Retry {
    fn initial_delay(&self) -> Duration {
        if let Some(delay) = &self.delay {
            match delay {
                Delay::Static { delay: duration } => *duration,
                Delay::ExponentialBackoff {
                    initial_delay,
                    max_delay: _,
                } => *initial_delay,
            }
        } else {
            Duration::ZERO
        }
    }

    fn next_delay(&self, _tries: usize, last_delay: Duration) -> Duration {
        if let Some(delay) = &self.delay {
            match delay {
                Delay::Static { delay: duration } => *duration,
                Delay::ExponentialBackoff {
                    initial_delay: _,
                    max_delay,
                } => {
                    let mut next = last_delay * 2;
                    if let Some(max_delay) = max_delay {
                        if next > *max_delay {
                            next = *max_delay;
                        }
                    }
                    next
                }
            }
        } else {
            last_delay
        }
    }

    fn is_exhausted(&self, tries: usize) -> bool {
        tries >= self.max_tries
    }
}
