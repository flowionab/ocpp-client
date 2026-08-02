use serde_json::Value;

/// Implemented once per OCPP version by that version's error enum (`OCPP1_6Error`,
/// `OCPP2_0_1Error`, ...), so the generic [`crate::Client`] engine can build and read
/// CALLERROR payloads without knowing which version it's carrying.
pub trait ProtocolError: core::fmt::Debug + Send + Sync + Sized + 'static {
    fn code(&self) -> &str;
    fn description(&self) -> &str;
    fn details(&self) -> &Value;
    fn not_implemented(action: &str) -> Self;
    fn from_wire(code: &str, description: &str, details: Value) -> Self;
}

/// Everything that can go wrong sending or receiving a single OCPP action, flattened into
/// one type instead of the `Result<Result<Response, ProtocolError>, Box<dyn Error>>` shape.
#[derive(Debug)]
pub enum ClientError<E> {
    /// The other side answered with a CALLERROR.
    Protocol(E),
    /// No CALLRESULT/CALLERROR arrived before the client's timeout elapsed.
    Timeout,
    /// The payload didn't match the expected request/response type.
    Decode(serde_json::Error),
    /// The transport failed to send or receive a frame.
    Transport(crate::transport::TransportError),
    /// The connection was closed before a response arrived.
    Closed,
}

impl<E: ProtocolError> core::fmt::Display for ClientError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ClientError::Protocol(e) => {
                write!(f, "protocol error: {} ({})", e.code(), e.description())
            }
            ClientError::Timeout => write!(f, "request timed out"),
            ClientError::Decode(e) => write!(f, "failed to decode payload: {e}"),
            ClientError::Transport(e) => write!(f, "transport error: {e}"),
            ClientError::Closed => write!(f, "connection closed"),
        }
    }
}

impl<E: ProtocolError> std::error::Error for ClientError<E> {}
