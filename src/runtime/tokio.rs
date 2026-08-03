//! tokio-backed `Executor`/`Timer` impls, used by `connect_1_6`/`connect_2_0_1` and the
//! crate's own tests. Gated behind the `tokio-runtime` feature.

use crate::runtime::{Executor, Timer};
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::time::Duration;

/// [`Executor`] impl that spawns onto the ambient tokio runtime via `tokio::spawn`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioExecutor;

impl Executor for TokioExecutor {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }
}

/// [`Timer`] impl backed by `tokio::time::sleep`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioTimer;

impl Timer for TokioTimer {
    fn delay<'a>(&'a self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(tokio::time::sleep(duration))
    }
}
