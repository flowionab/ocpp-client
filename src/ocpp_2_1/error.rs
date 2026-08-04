use crate::error::ProtocolError;
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use ocpp_types::v21::RpcErrorCode;
use serde_json::{Value, json};

/// An OCPP 2.1 CALLERROR: the RPC framework error code from
/// [`ocpp_types::v21::RpcErrorCode`], paired with the free-text description and details a
/// CALLERROR frame carries alongside it. The OCPP-J RPC framework error codes are unchanged
/// between 2.0.1 and 2.1, so this mirrors `OCPP2_0_1Error` exactly.
#[derive(Debug, Clone)]
pub struct OCPP2_1Error {
    pub code: RpcErrorCode,
    pub description: String,
    pub details: Value,
}

/// The exact wire spelling for each code, per the OCPP-J 2.1 specification's RPC framework
/// error code table - `RpcErrorCode`'s serde impl already produces this, but
/// `ProtocolError::code` returns a borrowed `&str` rather than allocating, so this mirrors it
/// as a `match` instead of going through serialization.
fn wire_code(code: RpcErrorCode) -> &'static str {
    match code {
        RpcErrorCode::FormatViolation => "FormatViolation",
        RpcErrorCode::GenericError => "GenericError",
        RpcErrorCode::InternalError => "InternalError",
        RpcErrorCode::MessageTypeNotSupported => "MessageTypeNotSupported",
        RpcErrorCode::NotImplemented => "NotImplemented",
        RpcErrorCode::NotSupported => "NotSupported",
        RpcErrorCode::OccurrenceConstraintViolation => "OccurrenceConstraintViolation",
        RpcErrorCode::PropertyConstraintViolation => "PropertyConstraintViolation",
        RpcErrorCode::ProtocolError => "ProtocolError",
        RpcErrorCode::RpcFrameworkError => "RpcFrameworkError",
        RpcErrorCode::SecurityError => "SecurityError",
        RpcErrorCode::TypeConstraintViolation => "TypeConstraintViolation",
    }
}

impl ProtocolError for OCPP2_1Error {
    fn code(&self) -> &str {
        wire_code(self.code)
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn details(&self) -> &Value {
        &self.details
    }

    fn not_implemented(action: &str) -> Self {
        OCPP2_1Error {
            code: RpcErrorCode::NotImplemented,
            description: format!("Action '{action}' is not implemented"),
            details: json!({}),
        }
    }

    fn from_wire(code: &str, description: &str, details: Value) -> Self {
        let code = match code {
            "FormatViolation" => RpcErrorCode::FormatViolation,
            "InternalError" => RpcErrorCode::InternalError,
            "MessageTypeNotSupported" => RpcErrorCode::MessageTypeNotSupported,
            "NotImplemented" => RpcErrorCode::NotImplemented,
            "NotSupported" => RpcErrorCode::NotSupported,
            "OccurrenceConstraintViolation" => RpcErrorCode::OccurrenceConstraintViolation,
            "PropertyConstraintViolation" => RpcErrorCode::PropertyConstraintViolation,
            "ProtocolError" => RpcErrorCode::ProtocolError,
            "RpcFrameworkError" => RpcErrorCode::RpcFrameworkError,
            "SecurityError" => RpcErrorCode::SecurityError,
            "TypeConstraintViolation" => RpcErrorCode::TypeConstraintViolation,
            _ => RpcErrorCode::GenericError,
        };
        OCPP2_1Error {
            code,
            description: description.to_string(),
            details,
        }
    }
}

impl fmt::Display for OCPP2_1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.description())
    }
}

impl core::error::Error for OCPP2_1Error {}
