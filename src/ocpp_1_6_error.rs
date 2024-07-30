use std::fmt::{self, Display};

use serde_json::{json, Value};
use crate::raw_ocpp_1_6_error::RawOcpp1_6Error;

#[derive(Debug, Clone)]
pub enum OCPP1_6Error {
    NotImplemented { description: String, details: Value },
    NotSupported { description: String, details: Value },
    InternalError { description: String, details: Value },
    ProtocolError { description: String, details: Value },
    SecurityError { description: String, details: Value },
    FormationViolation { description: String, details: Value },
    PropertyConstraintViolation { description: String, details: Value },
    OccurenceConstraintViolation { description: String, details: Value },
    TypeConstraintViolation { description: String, details: Value },
    GenericError { description: String, details: Value },
}

impl OCPP1_6Error {
    pub fn new_not_implemented(description: &str) -> Self {
        OCPP1_6Error::NotImplemented {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn new_security_error(description: &str) -> Self {
        OCPP1_6Error::SecurityError {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn new_formation_violation(description: &str) -> Self {
        OCPP1_6Error::FormationViolation {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn new_internal<T: Display>(error: &T) -> Self {
        OCPP1_6Error::InternalError {
            description: error.to_string(),
            details: json!({}),
        }
    }

    pub fn new_internal_str(description: &str) -> Self {
        OCPP1_6Error::InternalError {
            description: description.to_string(),
            details: json!({}),
        }
    }

    pub fn code(&self) -> &str {
        match self {
            OCPP1_6Error::NotImplemented {
                description: _,
                details: _,
            } => "NotImplemented",
            OCPP1_6Error::NotSupported {
                description: _,
                details: _,
            } => "NotSupported",
            OCPP1_6Error::InternalError {
                description: _,
                details: _,
            } => "InternalError",
            OCPP1_6Error::ProtocolError {
                description: _,
                details: _,
            } => "ProtocolError",
            OCPP1_6Error::SecurityError {
                description: _,
                details: _,
            } => "SecurityError",
            OCPP1_6Error::FormationViolation {
                description: _,
                details: _,
            } => "FormationViolation",
            OCPP1_6Error::PropertyConstraintViolation {
                description: _,
                details: _,
            } => "PropertyConstraintViolation",
            OCPP1_6Error::OccurenceConstraintViolation {
                description: _,
                details: _,
            } => "OccurenceConstraintViolation",
            OCPP1_6Error::TypeConstraintViolation {
                description: _,
                details: _,
            } => "TypeConstraintViolation",
            OCPP1_6Error::GenericError {
                description: _,
                details: _,
            } => "GenericError",
        }
    }

    pub fn description(&self) -> &str {
        match self {
            OCPP1_6Error::NotImplemented {
                description,
                details: _,
            } => description,
            OCPP1_6Error::NotSupported {
                description,
                details: _,
            } => description,
            OCPP1_6Error::InternalError {
                description,
                details: _,
            } => description,
            OCPP1_6Error::ProtocolError {
                description,
                details: _,
            } => description,
            OCPP1_6Error::SecurityError {
                description,
                details: _,
            } => description,
            OCPP1_6Error::FormationViolation {
                description,
                details: _,
            } => description,
            OCPP1_6Error::PropertyConstraintViolation {
                description,
                details: _,
            } => description,
            OCPP1_6Error::OccurenceConstraintViolation {
                description,
                details: _,
            } => description,
            OCPP1_6Error::TypeConstraintViolation {
                description,
                details: _,
            } => description,
            OCPP1_6Error::GenericError {
                description,
                details: _,
            } => description,
        }
    }

    pub fn details(&self) -> &Value {
        match self {
            OCPP1_6Error::NotImplemented {
                description: _,
                details,
            } => details,
            OCPP1_6Error::NotSupported {
                description: _,
                details,
            } => details,
            OCPP1_6Error::InternalError {
                description: _,
                details,
            } => details,
            OCPP1_6Error::ProtocolError {
                description: _,
                details,
            } => details,
            OCPP1_6Error::SecurityError {
                description: _,
                details,
            } => details,
            OCPP1_6Error::FormationViolation {
                description: _,
                details,
            } => details,
            OCPP1_6Error::PropertyConstraintViolation {
                description: _,
                details,
            } => details,
            OCPP1_6Error::OccurenceConstraintViolation {
                description: _,
                details,
            } => details,
            OCPP1_6Error::TypeConstraintViolation {
                description: _,
                details,
            } => details,
            OCPP1_6Error::GenericError {
                description: _,
                details,
            } => details,
        }
    }
}

impl fmt::Display for OCPP1_6Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OCPP1_6Error::NotImplemented {
                description,
                details,
            } => write!(
                f,
                "NotImplemented: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::NotSupported {
                description,
                details,
            } => write!(
                f,
                "NotSupported: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::InternalError {
                description,
                details,
            } => write!(
                f,
                "InternalError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::ProtocolError {
                description,
                details,
            } => write!(
                f,
                "ProtocolError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::SecurityError {
                description,
                details,
            } => write!(
                f,
                "SecurityError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::FormationViolation {
                description,
                details,
            } => write!(
                f,
                "FormationViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::PropertyConstraintViolation {
                description,
                details,
            } => write!(
                f,
                "PropertyConstraintViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::OccurenceConstraintViolation {
                description,
                details,
            } => write!(
                f,
                "OccurenceConstraintViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::TypeConstraintViolation {
                description,
                details,
            } => write!(
                f,
                "TypeConstraintViolation: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
            OCPP1_6Error::GenericError {
                description,
                details,
            } => write!(
                f,
                "GenericError: {} - {}",
                description,
                serde_json::to_string_pretty(details).unwrap()
            ),
        }
    }
}

impl std::error::Error for OCPP1_6Error {}

impl From<RawOcpp1_6Error> for OCPP1_6Error {
    fn from(_value: RawOcpp1_6Error) -> Self {
        todo!()
    }
}