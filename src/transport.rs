#[cfg(feature = "websocket")]
pub(crate) mod websocket;

use alloc::boxed::Box;
use alloc::string::String;
use core::future::Future;
use core::pin::Pin;

/// A transport-agnostic boxed error, so `TransportSink`/`TransportStream` stay dyn-safe
/// regardless of what's underneath (WebSocket today, a framed serial link later).
pub type TransportError = Box<dyn core::error::Error + Send + Sync>;

/// One thing read off a transport: a complete OCPP-J text frame, or a protocol-level
/// keepalive event. Carrying ping/pong through the abstraction (rather than hiding it
/// entirely inside the WebSocket adapter) keeps `send_ping`/`on_ping` possible without the
/// generic client knowing anything WebSocket-specific.
#[derive(Debug)]
pub enum TransportEvent {
    Frame(String),
    Ping,
    Pong,
}

/// The write half of a transport: sends one complete OCPP-J text frame at a time.
///
/// Implementations own only framing (e.g. WebSocket masking) - `Client` never sees
/// anything but whole frames and keepalive events.
///
/// Methods return a boxed future (the shape `#[async_trait]` expands to, written by hand)
/// rather than using `async fn` in the trait, so `Box<dyn TransportSink>` stays usable - this
/// crate has no dependency on the `async-trait` crate itself, only on `alloc`.
pub trait TransportSink: Send {
    fn send<'a>(
        &'a mut self,
        frame: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
    fn ping<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
    fn pong<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
}

/// The read half of a transport: yields one [`TransportEvent`] at a time, or `None` when
/// the other side closed the connection.
pub trait TransportStream: Send {
    fn recv<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<TransportEvent>, TransportError>> + Send + 'a>>;
}
