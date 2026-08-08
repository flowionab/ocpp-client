use crate::error::ProtocolError;
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use ocpp_types::v201::RpcErrorCode;
use serde_json::{Value, json};

/// An OCPP 2.0.1 CALLERROR: the RPC framework error code from
/// [`ocpp_types::v201::RpcErrorCode`], paired with the free-text description and details a
/// CALLERROR frame carries alongside it (the spec leaves `errorDetails`'s shape undefined, so
/// this crate keeps it as a raw `Value` rather than a typed field).
#[derive(Debug, Clone)]
pub struct OCPP2_0_1Error {
    pub code: RpcErrorCode,
    pub description: String,
    pub details: Value,
}

/// The exact wire spelling for each code, per the OCPP-J 2.0.1 specification's RPC framework
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

impl ProtocolError for OCPP2_0_1Error {
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
        OCPP2_0_1Error {
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
        OCPP2_0_1Error {
            code,
            description: description.to_string(),
            details,
        }
    }
}

/// Answer a schema violation with the CALLERROR 2.0.1 names for it. Unlike 1.6J, 2.x spells
/// `OccurrenceConstraintViolation` with both `r`s - see the 1.6 impl for why that matters.
#[cfg(feature = "validate")]
impl From<ocpp_types::validate::ValidationError> for OCPP2_0_1Error {
    fn from(error: ocpp_types::validate::ValidationError) -> Self {
        use ocpp_types::validate::ConstraintClass;

        let code = match error.kind().constraint_class() {
            ConstraintClass::Property => RpcErrorCode::PropertyConstraintViolation,
            ConstraintClass::Occurrence => RpcErrorCode::OccurrenceConstraintViolation,
        };
        let (description, details) = crate::error::validation_error_parts(&error);
        OCPP2_0_1Error {
            code,
            description,
            details,
        }
    }
}

impl fmt::Display for OCPP2_0_1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.description())
    }
}

impl core::error::Error for OCPP2_0_1Error {}
