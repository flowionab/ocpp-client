#[cfg(feature = "websocket")]
pub(crate) mod websocket;

/// A transport-agnostic boxed error, so `TransportSink`/`TransportStream` stay dyn-safe
/// regardless of what's underneath (WebSocket today, a framed serial link later).
pub type TransportError = Box<dyn std::error::Error + Send + Sync>;

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
#[async_trait::async_trait]
pub trait TransportSink: Send {
    async fn send(&mut self, frame: String) -> Result<(), TransportError>;
    async fn ping(&mut self) -> Result<(), TransportError>;
    async fn pong(&mut self) -> Result<(), TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}

/// The read half of a transport: yields one [`TransportEvent`] at a time, or `None` when
/// the other side closed the connection.
#[async_trait::async_trait]
pub trait TransportStream: Send {
    async fn recv(&mut self) -> Result<Option<TransportEvent>, TransportError>;
}
