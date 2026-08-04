use crate::error::ProtocolError;
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use ocpp_types::v16::RpcErrorCode;
use serde_json::{Value, json};

/// An OCPP 1.6 CALLERROR: the RPC framework error code from
/// [`ocpp_types::v16::RpcErrorCode`], paired with the free-text description and details a
/// CALLERROR frame carries alongside it (the spec leaves `errorDetails`'s shape undefined, so
/// this crate keeps it as a raw `Value` rather than a typed field).
#[derive(Debug, Clone)]
pub struct OCPP1_6Error {
    pub code: RpcErrorCode,
    pub description: String,
    pub details: Value,
}

/// The exact wire spelling for each code, per Table 7 of the OCPP-J 1.6 specification -
/// `RpcErrorCode`'s serde impl already produces this, but `ProtocolError::code` returns a
/// borrowed `&str` rather than allocating, so this mirrors it as a `match` instead of going
/// through serialization.
fn wire_code(code: RpcErrorCode) -> &'static str {
    match code {
        RpcErrorCode::NotImplemented => "NotImplemented",
        RpcErrorCode::NotSupported => "NotSupported",
        RpcErrorCode::InternalError => "InternalError",
        RpcErrorCode::ProtocolError => "ProtocolError",
        RpcErrorCode::SecurityError => "SecurityError",
        RpcErrorCode::FormationViolation => "FormationViolation",
        RpcErrorCode::PropertyConstraintViolation => "PropertyConstraintViolation",
        RpcErrorCode::OccurenceConstraintViolation => "OccurenceConstraintViolation",
        RpcErrorCode::TypeConstraintViolation => "TypeConstraintViolation",
        RpcErrorCode::GenericError => "GenericError",
    }
}

impl ProtocolError for OCPP1_6Error {
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
        OCPP1_6Error {
            code: RpcErrorCode::NotImplemented,
            description: format!("Action '{action}' is not implemented"),
            details: json!({}),
        }
    }

    fn from_wire(code: &str, description: &str, details: Value) -> Self {
        let code = match code {
            "NotImplemented" => RpcErrorCode::NotImplemented,
            "NotSupported" => RpcErrorCode::NotSupported,
            "InternalError" => RpcErrorCode::InternalError,
            "ProtocolError" => RpcErrorCode::ProtocolError,
            "SecurityError" => RpcErrorCode::SecurityError,
            "FormationViolation" => RpcErrorCode::FormationViolation,
            "PropertyConstraintViolation" => RpcErrorCode::PropertyConstraintViolation,
            "OccurenceConstraintViolation" => RpcErrorCode::OccurenceConstraintViolation,
            "TypeConstraintViolation" => RpcErrorCode::TypeConstraintViolation,
            _ => RpcErrorCode::GenericError,
        };
        OCPP1_6Error {
            code,
            description: description.to_string(),
            details,
        }
    }
}

impl fmt::Display for OCPP1_6Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.description())
    }
}

impl core::error::Error for OCPP1_6Error {}
