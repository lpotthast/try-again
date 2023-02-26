pub trait DelayStrategy<D> {
    type Out;

    fn delay(&self, by: D) -> Self::Out;
}

#[derive(Debug, Clone, Copy)]
pub struct ThreadSleep {}

impl<D: Into<std::time::Duration>> DelayStrategy<D> for ThreadSleep {
    type Out = ();

    fn delay(&self, delay: D) -> Self::Out {
        std::thread::sleep(delay.into())
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg(feature = "async-tokio")]
pub struct TokioSleep {}

#[cfg(feature = "async-tokio")]
impl<D: Into<std::time::Duration>> DelayStrategy<D> for TokioSleep {
    type Out = tokio::time::Sleep;

    fn delay(&self, delay: D) -> Self::Out {
        tokio::time::sleep(delay.into())
    }
}
