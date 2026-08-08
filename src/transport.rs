#[cfg(feature = "websocket")]
pub(crate) mod websocket;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;

/// A transport-agnostic boxed error, so `TransportSink`/`TransportStream` stay dyn-safe
/// regardless of what's underneath (WebSocket today, a framed serial link later).
pub type TransportError = Box<dyn core::error::Error + Send + Sync>;

/// One thing read off a transport: a complete OCPP-J text frame, or a protocol-level
/// keepalive event. Carrying ping/pong through the abstraction (rather than hiding it
/// entirely inside the WebSocket adapter) keeps `send_ping`/`on_ping` possible without the
/// generic client knowing anything WebSocket-specific.
///
/// `Ping`/`Pong` carry the frame's application data, which RFC 6455 §5.5.2-3 requires a pong
/// to echo back from the ping that triggered it. `Client` relies on that echo to match a pong
/// to the exact `send_ping` waiting for it, so transport implementations must pass the payload
/// through rather than discarding it.
#[derive(Debug)]
pub enum TransportEvent {
    Frame(String),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
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
    /// Send a ping carrying `payload` as its application data. `Client` puts a correlation
    /// token there and matches it against the echoed payload of the pong that comes back, so
    /// implementations must transmit it verbatim instead of sending an empty ping.
    fn ping<'a>(
        &'a mut self,
        payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
    /// Send a pong carrying `payload`. When replying to a received ping, RFC 6455 requires
    /// this to be that ping's application data verbatim - the read loop passes it straight
    /// through from [`TransportEvent::Ping`].
    fn pong<'a>(
        &'a mut self,
        payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
}

/// The read half of a transport: yields one [`TransportEvent`] at a time, or `None` when
/// the other side closed the connection.
pub trait TransportStream: Send {
    /// Yield the next event.
    ///
    /// **Must be cancel-safe**: the returned future can be dropped before it completes, and
    /// doing so must not lose or partially consume an event - the next `recv` call has to pick
    /// up where this one left off. `Client`'s read loop races this against an internal
    /// "abandon this connection and redial" signal (fired by keepalive when the peer stops
    /// answering pings), so a stalled `recv` gets dropped mid-poll rather than parking the loop
    /// until the OS TCP timeout.
    ///
    /// The two implementations in the Flowion tree satisfy this because they bottom out in
    /// already-cancel-safe primitives (`futures::StreamExt::next` over a `tokio-tungstenite`
    /// stream; an `embassy-net` socket read). An implementation that buffers partial state in a
    /// local variable across an `.await` needs to move that state into `self` to qualify.
    fn recv<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<TransportEvent>, TransportError>> + Send + 'a>>;
}
