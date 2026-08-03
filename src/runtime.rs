//! Runtime abstraction so the engine (`Client<E>`) doesn't hard-depend on tokio. `Executor`
//! spawns the background read loop and per-handler tasks; `Timer` drives request/ping
//! timeouts. Both are dyn-safe (boxed-future style) so `Client<E>` stays generic over one
//! type parameter only, the same way `TransportSink`/`TransportStream` are already boxed
//! instead of threaded through as generics. `tokio-runtime` (see `runtime::tokio`) provides
//! the default std impls; embedded users supply their own (e.g. backed by
//! `embassy-executor`/`embassy-time`).

use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::task::Poll;
use core::time::Duration;

#[cfg(feature = "tokio-runtime")]
pub mod tokio;

/// Spawns futures onto a background executor. Implementations must actually run the future
/// to completion independently of the caller awaiting anything - `Client::from_transport`'s
/// read loop, `on()`'s per-action handler loop, and `on_ping()`'s subscriber loop all rely on
/// `spawn` to keep running in the background.
pub trait Executor: Send + Sync + 'static {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>);
}

/// Produces timer delays. `with_timeout` (below) is built on top of this single dyn-safe
/// method rather than a generic `timeout<F>` method, so `Timer` itself stays object-safe.
pub trait Timer: Send + Sync + 'static {
    fn delay<'a>(&'a self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// Returned by [`with_timeout`] when `duration` elapses before `fut` resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elapsed;

/// Race `fut` against `timer.delay(duration)`, by hand - no `futures::select`/extra
/// dependency needed, just polling both each wake via `core::future::poll_fn`.
pub(crate) async fn with_timeout<F: Future>(
    timer: &dyn Timer,
    duration: Duration,
    fut: F,
) -> Result<F::Output, Elapsed> {
    let mut fut = core::pin::pin!(fut);
    let mut delay = timer.delay(duration);
    core::future::poll_fn(move |cx| {
        if let Poll::Ready(value) = fut.as_mut().poll(cx) {
            return Poll::Ready(Ok(value));
        }
        if let Poll::Ready(()) = delay.as_mut().poll(cx) {
            return Poll::Ready(Err(Elapsed));
        }
        Poll::Pending
    })
    .await
}
