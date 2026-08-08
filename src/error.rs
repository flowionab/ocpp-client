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

impl<E: ProtocolError> core::error::Error for ClientError<E> {}

/// The version-independent half of each version's `From<ValidationError>` impl: the CALLERROR
/// `description` and `errorDetails` that answer a schema violation.
///
/// Only the wire *code* differs between versions - 1.6J's `OccurenceConstraintViolation` against
/// 2.x's `OccurrenceConstraintViolation` - so the three impls pick that themselves from
/// [`ocpp_types::validate::ConstraintClass`] and share everything else through here.
///
/// `errorDetails` has no shape in the specification, so it carries the JSON path on its own,
/// separately from the sentence in `description`: a peer can match on `details["path"]` without
/// parsing prose. The path renders the same way `ValidationError`'s `Display` writes it
/// (`id[0]`, and `<payload>` for a violation at the root), because it *is* that rendering minus
/// the trailing reason.
///
/// The version features are part of the gate, not just `validate`: every caller lives in a
/// per-version module, so `validate` on its own - which is a legitimate thing for a consumer to
/// select - would leave this dead and fail the `-D warnings` clippy. `--all-features` never
/// reaches that combination.
#[cfg(all(
    feature = "validate",
    any(feature = "ocpp_1_6", feature = "ocpp_2_0_1", feature = "ocpp_2_1")
))]
pub(crate) fn validation_error_parts(
    error: &ocpp_types::validate::ValidationError,
) -> (alloc::string::String, Value) {
    use alloc::string::ToString;
    use core::fmt::Write;
    use ocpp_types::validate::PathSegment;

    let mut path = alloc::string::String::new();
    if error.path_truncated() {
        path.push_str("...");
    }
    if error.path().is_empty() && !error.path_truncated() {
        path.push_str("<payload>");
    }
    for (position, segment) in error.path().iter().enumerate() {
        match segment {
            PathSegment::Field(name) => {
                if position > 0 || error.path_truncated() {
                    path.push('.');
                }
                path.push_str(name);
            }
            // Writing into a String cannot fail; the Result is core::fmt's, not io's.
            PathSegment::Index(index) => {
                let _ = write!(path, "[{index}]");
            }
        }
    }

    (error.to_string(), serde_json::json!({ "path": path }))
}
