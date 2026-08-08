//! no_std+alloc-friendly replacements for the `tokio::sync::{Mutex,oneshot,mpsc,broadcast}`
//! primitives `client.rs` used to depend on directly. Built on `embassy-sync`'s `Mutex`/
//! `Signal`, fixed to `CriticalSectionRawMutex` (works under both std, via
//! `critical-section`'s `std` backend, and embedded, via whatever backend the embedded app
//! registers with `critical_section::set_impl!`) so `Client<E>` doesn't need a third generic
//! parameter for "which raw mutex".

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use embassy_sync::signal::Signal;

/// Direct replacement for `tokio::sync::Mutex<T>` - same `.lock().await` shape.
pub(crate) type SharedMutex<T> = EmbassyMutex<CriticalSectionRawMutex, T>;

/// Single-value handoff between one sender and one receiver, replacing
/// `tokio::sync::oneshot::{Sender,Receiver}`. `send`/`wait` can be called from independent
/// clones (both hold the same underlying `Signal`), so the same handle can be stored in a
/// lookup map for the sender side while the caller keeps its own clone to await on.
pub(crate) struct OneShot<T> {
    signal: Arc<Signal<CriticalSectionRawMutex, T>>,
}

impl<T> OneShot<T> {
    pub(crate) fn new() -> Self {
        Self {
            signal: Arc::new(Signal::new()),
        }
    }

    pub(crate) fn send(&self, value: T) {
        self.signal.signal(value);
    }

    pub(crate) async fn wait(&self) -> T {
        self.signal.wait().await
    }
}

impl<T> Clone for OneShot<T> {
    fn clone(&self) -> Self {
        Self {
            signal: self.signal.clone(),
        }
    }
}

struct ChanInner<T> {
    queue: SharedMutex<VecDeque<T>>,
    signal: Signal<CriticalSectionRawMutex, ()>,
    closed: AtomicBool,
}

/// Unbounded multi-producer, single-consumer-ish queue replacing the bounded
/// `tokio::sync::mpsc::channel(1000)` used for dispatching incoming CALLs to a registered
/// handler. Backed by `alloc::collections::VecDeque` instead of a fixed-capacity ring buffer -
/// unbounded, which suits no_std better than reserving a large fixed slot count up front.
pub(crate) struct Chan<T> {
    inner: Arc<ChanInner<T>>,
}

impl<T> Chan<T> {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(ChanInner {
                queue: SharedMutex::new(VecDeque::new()),
                signal: Signal::new(),
                closed: AtomicBool::new(false),
            }),
        }
    }

    pub(crate) async fn send(&self, value: T) {
        {
            let mut queue = self.inner.queue.lock().await;
            queue.push_back(value);
        }
        self.inner.signal.signal(());
    }

    /// Stop this channel: once whatever is already queued has been drained, `recv` returns
    /// `None` instead of parking forever.
    ///
    /// This exists so a handler task can be retired. `Client::on` replaces the registered
    /// channel for an action, which leaves the previous task blocked on a channel nothing can
    /// ever deliver to - it would loop forever, one leaked task per re-registration. Closing the
    /// channel it holds is what lets it end.
    pub(crate) fn close(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        self.inner.signal.signal(());
    }

    /// Waits for the next queued value, or `None` once the channel is closed and drained.
    ///
    /// `Signal`'s "latch" semantics (a `signal()` call before `wait()` is still observed) make
    /// the check-queue-then-wait-then-retry loop race-free, and mean a `close` racing a `send`
    /// still wakes the receiver exactly once - it re-reads both the queue and the flag on each
    /// wake, so a coalesced signal loses nothing.
    ///
    /// Draining before reporting closed is deliberate: a retired handler still answers the calls
    /// that were already dispatched to it, rather than leaving the peer without a reply.
    pub(crate) async fn recv(&self) -> Option<T> {
        loop {
            {
                let mut queue = self.inner.queue.lock().await;
                if let Some(value) = queue.pop_front() {
                    return Some(value);
                }
                if self.inner.closed.load(Ordering::SeqCst) {
                    return None;
                }
            }
            self.inner.signal.wait().await;
        }
    }

    /// Whether two handles refer to the same channel, so a caller can tell "the registration I
    /// made" from "a registration someone else replaced it with".
    ///
    /// Only `wait_for` needs this, and that is `test`-gated - without the same gate this is dead
    /// code in every ordinary build, which the embedded CI job (`clippy -D warnings`) rejects.
    #[cfg(feature = "test")]
    pub(crate) fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<T> Clone for Chan<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Single-consumer, repeatable wakeup. Unlike [`OneShot`], `wait` is meant to be called in a
/// loop: `embassy_sync`'s `Signal` resets itself once its value is taken, so each `notify`
/// releases exactly one subsequent `wait`. Latching (a `notify` before the matching `wait` is
/// still observed) means the notifier never has to know whether the waiter is parked yet.
///
/// Used for the two "wake this loop up now" edges the keepalive machinery needs: telling the
/// keepalive task its interval was reconfigured (`Client::set_ping_interval`), and telling the
/// read loop to abandon the current transport and redial (`Client::force_reconnect`).
pub(crate) struct Notify {
    signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
}

impl Notify {
    pub(crate) fn new() -> Self {
        Self {
            signal: Arc::new(Signal::new()),
        }
    }

    pub(crate) fn notify(&self) {
        self.signal.signal(());
    }

    pub(crate) async fn wait(&self) {
        self.signal.wait().await
    }
}

impl Clone for Notify {
    fn clone(&self) -> Self {
        Self {
            signal: self.signal.clone(),
        }
    }
}

/// Fan-out notification registry replacing `tokio::sync::broadcast`, used for `send_ping`'s
/// `Ping` event notifying every `on_ping` subscriber. Each subscriber gets its own single-slot
/// `Signal`, so (unlike tokio's broadcast, which buffers) a subscriber that hasn't consumed the
/// previous ping just sees the latest one rather than a queue of every ping - acceptable for a
/// low-frequency keepalive signal.
pub(crate) struct BroadcastRegistry {
    subscribers: SharedMutex<Vec<Arc<Signal<CriticalSectionRawMutex, ()>>>>,
}

impl BroadcastRegistry {
    pub(crate) fn new() -> Self {
        Self {
            subscribers: SharedMutex::new(Vec::new()),
        }
    }

    pub(crate) async fn subscribe(&self) -> Arc<Signal<CriticalSectionRawMutex, ()>> {
        let signal = Arc::new(Signal::new());
        let mut subscribers = self.subscribers.lock().await;
        subscribers.push(signal.clone());
        signal
    }

    pub(crate) async fn notify_all(&self) {
        let subscribers = self.subscribers.lock().await;
        for signal in subscribers.iter() {
            signal.signal(());
        }
    }
}
