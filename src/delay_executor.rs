use crate::StdDuration;
use std::fmt::Debug;

pub trait DelayExecutor<Delay>: Debug {
    fn delay_by(&self, by: Delay);
}

#[cfg(feature = "async")]
pub trait AsyncDelayExecutor<Delay>: Debug {
    #[allow(async_fn_in_trait)]
    async fn delay_by(&self, by: Delay);
}

#[derive(Debug, Clone, Copy)]
pub struct ThreadSleep;

impl<Delay: Into<StdDuration>> DelayExecutor<Delay> for ThreadSleep {
    fn delay_by(&self, delay: Delay) {
        std::thread::sleep(delay.into())
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg(feature = "async-tokio")]
pub struct TokioSleep;

#[cfg(feature = "async-tokio")]
impl<Delay: Into<StdDuration>> AsyncDelayExecutor<Delay> for TokioSleep {
    async fn delay_by(&self, delay: Delay) {
        tokio::time::sleep(delay.into()).await
    }
}
