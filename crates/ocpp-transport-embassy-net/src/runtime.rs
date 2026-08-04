//! `ocpp_client::Executor`/`Timer` implementations backed by `embassy-executor`/`embassy-time`.
//!
//! `Executor::spawn` takes an arbitrary `Pin<Box<dyn Future<Output = ()> + Send>>`, but
//! `embassy-executor` tasks are normally defined via `#[embassy_executor::task]` with static
//! storage sized at compile time - there's no built-in "spawn any boxed future" primitive. The
//! standard workaround (used here) is a fixed-size task pool: one `#[task(pool_size = N)]`
//! function whose only job is to poll whatever boxed future it's handed, giving us up to `N`
//! concurrent boxed-future tasks from N statically-sized slots.
//!
//! `Client<E>` (from `ocpp-client`) spawns one background read-loop task for the lifetime of
//! the connection, plus one additional task per `on()`/`on_ping()`/`on_reconnect()`
//! registration. Size [`POOL_SIZE`] to (1 + number of handlers you register) with a little
//! headroom; `spawn` logs via `tracing::error!` and silently drops the future if the pool is
//! exhausted (the `Executor` trait's `spawn` has no way to report failure to the caller).

use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::time::Duration;
use ocpp_client::{Executor, Timer};

/// Number of concurrent boxed-future tasks this executor can run at once. Must match the
/// literal `pool_size` passed to `#[embassy_executor::task]` below - the macro needs a literal
/// (or at least a const-expression that doesn't reference another item in the same module;
/// pointing it at this const causes a "cycle detected checking if POOL_SIZE is a trivial const"
/// error) so the two can't just share one definition. If you bump one, bump the other.
pub const POOL_SIZE: usize = 8;

type BoxedFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[embassy_executor::task(pool_size = 8)]
async fn run_boxed_future(future: BoxedFuture) {
    future.await;
}

/// `embassy_executor::Spawner` is `!Send`/`!Sync` by design (it holds a `PhantomData<*mut ()>`
/// marker) - embassy's execution model is a single, non-preemptive executor per core, so
/// there's no real concurrent access to guard against and the type just doesn't bother
/// implementing the auto-traits. `ocpp_client::Executor: Send + Sync + 'static` was written
/// with a multi-threaded runtime (tokio) in mind and has no escape hatch for "single-core,
/// cooperative, nothing is ever truly concurrent" targets, so this wrapper unsafely asserts
/// what's already true for that execution model.
///
/// # Safety
/// Sound only under embassy's normal usage model: a single, non-preemptive executor on one
/// core, where every `.await` point is the only place execution can hand off - two pieces of
/// code never run `Spawner` methods at the *same instant*, only interleaved. This does **not**
/// hold on a multi-core target (e.g. running one executor per core and sharing a `Spawner`
/// across them) or anywhere with genuine preemption - don't reuse this wrapper there.
#[derive(Clone, Copy)]
struct AssertSendSync<T>(T);
unsafe impl<T> Send for AssertSendSync<T> {}
unsafe impl<T> Sync for AssertSendSync<T> {}

/// `ocpp_client::Executor` backed by an `embassy_executor::Spawner` and the fixed-size task
/// pool above.
#[derive(Clone)]
pub struct EmbassyExecutor {
    spawner: AssertSendSync<embassy_executor::Spawner>,
}

impl EmbassyExecutor {
    pub fn new(spawner: embassy_executor::Spawner) -> Self {
        Self {
            spawner: AssertSendSync(spawner),
        }
    }
}

impl Executor for EmbassyExecutor {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        // `run_boxed_future(future)` (not `Spawner::spawn`) is what can fail here - it's the
        // pool_size-backed task constructor that finds (or fails to find) a free static slot;
        // `Spawner::spawn` itself just enqueues an already-valid token and can't fail.
        match run_boxed_future(future) {
            Ok(token) => self.spawner.0.spawn(token),
            Err(_err) => {
                tracing::error!(
                    pool_size = POOL_SIZE,
                    "ocpp-transport-embassy-net: task pool exhausted, dropping spawned future - \
                     increase runtime::POOL_SIZE"
                );
            }
        }
    }
}

/// `ocpp_client::Timer` backed by `embassy_time::Timer::after_micros`.
#[derive(Clone, Copy, Default)]
pub struct EmbassyTimer;

impl Timer for EmbassyTimer {
    fn delay<'a>(&'a self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            embassy_time::Timer::after_micros(duration.as_micros() as u64).await;
        })
    }
}
