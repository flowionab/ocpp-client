//! no_std+alloc-friendly replacements for the `tokio::sync::{Mutex,oneshot,mpsc,broadcast}`
//! primitives `client.rs` used to depend on directly. Built on `embassy-sync`'s `Mutex`/
//! `Signal`, fixed to `CriticalSectionRawMutex` (works under both std, via
//! `critical-section`'s `std` backend, and embedded, via whatever backend the embedded app
//! registers with `critical_section::set_impl!`) so `Client<E>` doesn't need a third generic
//! parameter for "which raw mutex".

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
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

    /// Waits for the next queued value. `Signal`'s "latch" semantics (a `signal()` call before
    /// `wait()` is still observed) make the check-queue-then-wait-then-retry loop race-free.
    pub(crate) async fn recv(&self) -> T {
        loop {
            {
                let mut queue = self.inner.queue.lock().await;
                if let Some(value) = queue.pop_front() {
                    return value;
                }
            }
            self.inner.signal.wait().await;
        }
    }
}

impl<T> Clone for Chan<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
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
