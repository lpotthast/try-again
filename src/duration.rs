pub type StdDuration = std::time::Duration;

pub trait IntoStdDuration {
    fn nanos(self) -> StdDuration;

    fn micros(self) -> StdDuration;

    fn millis(self) -> StdDuration;

    fn secs(self) -> StdDuration;
}

impl IntoStdDuration for u64 {
    #[must_use]
    fn nanos(self) -> StdDuration {
        StdDuration::from_nanos(self)
    }

    #[must_use]
    fn micros(self) -> StdDuration {
        StdDuration::from_micros(self)
    }

    #[must_use]
    fn millis(self) -> StdDuration {
        StdDuration::from_millis(self)
    }

    #[must_use]
    fn secs(self) -> StdDuration {
        StdDuration::from_secs(self)
    }
}
