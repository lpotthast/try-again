pub type StdDuration = std::time::Duration;

pub trait IntoStdDuration {
    #[must_use]
    fn nanos(self) -> StdDuration;

    #[must_use]
    fn micros(self) -> StdDuration;

    #[must_use]
    fn millis(self) -> StdDuration;

    #[must_use]
    fn secs(self) -> StdDuration;
}

impl IntoStdDuration for u64 {
    fn nanos(self) -> StdDuration {
        StdDuration::from_nanos(self)
    }

    fn micros(self) -> StdDuration {
        StdDuration::from_micros(self)
    }

    fn millis(self) -> StdDuration {
        StdDuration::from_millis(self)
    }

    fn secs(self) -> StdDuration {
        StdDuration::from_secs(self)
    }
}
