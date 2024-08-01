use std::fmt;
use std::fmt::Display;
use serde_json::{json, Value};
use crate::ocpp_2_0_1::raw_ocpp_2_0_1_error::RawOcpp2_0_1Error;

/// Represents an OCPP 2.0.1 error
#[derive(Debug, Clone)]
pub enum OCPP2_0_1Error {
    FormatViolation { description: String, details: Value },
    GenericError { description: String, details: Value },
    InternalError { description: String, details: Value },
    MessageTypeNotSupported { description: String, details: Value },
    NotImplemented { description: String, details: Value },
    NotSupported { description: String, details: Value },
    OccurrenceConstraintViolation { description: String, details: Value },
    PropertyConstraintViolation { description: String, details: Value },
    ProtocolError { description: String, details: Value },
    RpcFrameworkError { description: String, details: Value },
    SecurityError { description: String, details: Value },
    TypeConstraintViolation { description: String, details: Value },
}


impl OCPP2_0_1Error {
    pub fn new_not_implemented(description: &str) -> Self {
        OCPP2_0_1Error::NotImplemented {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn new_security_error(description: &str) -> Self {
        OCPP2_0_1Error::SecurityError {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn new_format_violation(description: &str) -> Self {
        OCPP2_0_1Error::FormatViolation {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn new_internal<T: Display>(error: &T) -> Self {
        OCPP2_0_1Error::InternalError {
            description: error.to_string(),
            details: json!({}),
        }
    }

    pub fn new_internal_str(description: &str) -> Self {
        OCPP2_0_1Error::InternalError {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn code(&self) -> &str {
        match self {
            OCPP2_0_1Error::NotImplemented {
                description: _,
                details: _,
            } => "NotImplemented",
            OCPP2_0_1Error::NotSupported {
                description: _,
                details: _,
            } => "NotSupported",
            OCPP2_0_1Error::InternalError {
                description: _,
                details: _,
            } => "InternalError",
            OCPP2_0_1Error::ProtocolError {
                description: _,
                details: _,
            } => "ProtocolError",
            OCPP2_0_1Error::SecurityError {
                description: _,
                details: _,
            } => "SecurityError",
            OCPP2_0_1Error::FormatViolation {
                description: _,
                details: _,
            } => "FormatViolation",
            OCPP2_0_1Error::PropertyConstraintViolation {
                description: _,
                details: _,
            } => "PropertyConstraintViolation",
            OCPP2_0_1Error::OccurrenceConstraintViolation {
                description: _,
                details: _,
            } => "OccurrenceConstraintViolation",
            OCPP2_0_1Error::TypeConstraintViolation {
                description: _,
                details: _,
            } => "TypeConstraintViolation",
            OCPP2_0_1Error::GenericError {
                description: _,
                details: _,
            } => "GenericError",
            OCPP2_0_1Error::MessageTypeNotSupported {
                description: _,
                details: _,
            } => "MessageTypeNotSupported",
            OCPP2_0_1Error::RpcFrameworkError {
                description: _,
                details: _,
            } => "RpcFrameworkError",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            OCPP2_0_1Error::NotImplemented {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::NotSupported {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::InternalError {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::ProtocolError {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::SecurityError {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::FormatViolation {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::PropertyConstraintViolation {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::OccurrenceConstraintViolation {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::TypeConstraintViolation {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::GenericError {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::MessageTypeNotSupported {
                description,
                details: _,
            } => description,
            OCPP2_0_1Error::RpcFrameworkError {
                description,
                details: _,
            } => description,
        }
    }

    pub fn details(&self) -> &Value {
        match self {
            OCPP2_0_1Error::NotImplemented {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::NotSupported {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::InternalError {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::ProtocolError {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::SecurityError {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::FormatViolation {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::PropertyConstraintViolation {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::OccurrenceConstraintViolation {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::TypeConstraintViolation {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::GenericError {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::MessageTypeNotSupported {
                description: _,
                details,
            } => details,
            OCPP2_0_1Error::RpcFrameworkError {
                description: _,
                details,
            } => details,
        }
    }
}

impl fmt::Display for OCPP2_0_1Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OCPP2_0_1Error::NotImplemented {
                description,
                details,
            } => write!(
                f,
                "NotImplemented: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::NotSupported {
                description,
                details,
            } => write!(
                f,
                "NotSupported: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::InternalError {
                description,
                details,
            } => write!(
                f,
                "InternalError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::ProtocolError {
                description,
                details,
            } => write!(
                f,
                "ProtocolError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::SecurityError {
                description,
                details,
            } => write!(
                f,
                "SecurityError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::FormatViolation {
                description,
                details,
            } => write!(
                f,
                "FormatViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::PropertyConstraintViolation {
                description,
                details,
            } => write!(
                f,
                "PropertyConstraintViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::OccurrenceConstraintViolation {
                description,
                details,
            } => write!(
                f,
                "OccurrenceConstraintViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::TypeConstraintViolation {
                description,
                details,
            } => write!(
                f,
                "TypeConstraintViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::GenericError {
                description,
                details,
            } => write!(
                f,
                "GenericError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::MessageTypeNotSupported {
                description,
                details,
            } => write!(
                f,
                "MessageTypeNotSupported: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP2_0_1Error::RpcFrameworkError {
                description,
                details,
            } => write!(
                f,
                "RpcFrameworkError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            )
        }
    }
}

impl std::error::Error for OCPP2_0_1Error {}

impl From<RawOcpp2_0_1Error> for OCPP2_0_1Error {
    fn from(value: RawOcpp2_0_1Error) -> Self {
        match value.2.as_str() {
            "NotImplemented" => {
                Self::NotImplemented {
                    description: value.3,
                    details: value.4
                }
            },
            "NotSupported" => {
                Self::NotSupported {
                    description: value.3,
                    details: value.4
                }
            },
            "InternalError" => {
                Self::InternalError {
                    description: value.3,
                    details: value.4
                }
            },
            "ProtocolError" => {
                Self::ProtocolError {
                    description: value.3,
                    details: value.4
                }
            },
            "SecurityError" => {
                Self::SecurityError {
                    description: value.3,
                    details: value.4
                }
            },
            "FormatViolation" => {
                Self::FormatViolation {
                    description: value.3,
                    details: value.4
                }
            },
            "PropertyConstraintViolation" => {
                Self::PropertyConstraintViolation {
                    description: value.3,
                    details: value.4
                }
            },
            "OccurrenceConstraintViolation" => {
                Self::OccurrenceConstraintViolation {
                    description: value.3,
                    details: value.4
                }
            },
            "TypeConstraintViolation" => {
                Self::TypeConstraintViolation {
                    description: value.3,
                    details: value.4
                }
            },
            "MessageTypeNotSupported" => {
                Self::MessageTypeNotSupported {
                    description: value.3,
                    details: value.4
                }
            },
            "RpcFrameworkError" => {
                Self::RpcFrameworkError {
                    description: value.3,
                    details: value.4
                }
            },
            "GenericError" => {
                Self::GenericError {
                    description: value.3,
                    details: value.4
                }
            },
            _ => {
                Self::GenericError {
                    description: value.3,
                    details: value.4
                }
            }
        }
    }
}