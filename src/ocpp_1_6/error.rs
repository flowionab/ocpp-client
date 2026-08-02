use crate::error::ProtocolError;
use serde_json::{Value, json};
use std::fmt;

macro_rules! define_error {
    ($($variant:ident => $code:literal),+ $(,)?) => {
        /// An OCPP 1.6 CALLERROR, one variant per error code defined by the spec.
        #[derive(Debug, Clone)]
        pub enum OCPP1_6Error {
            $(
                #[doc = $code]
                $variant { description: String, details: Value }
            ),+
        }

        impl ProtocolError for OCPP1_6Error {
            fn code(&self) -> &str {
                match self {
                    $(OCPP1_6Error::$variant { .. } => $code),+
                }
            }

            fn description(&self) -> &str {
                match self {
                    $(OCPP1_6Error::$variant { description, .. } => description),+
                }
            }

            fn details(&self) -> &Value {
                match self {
                    $(OCPP1_6Error::$variant { details, .. } => details),+
                }
            }

            fn not_implemented(action: &str) -> Self {
                OCPP1_6Error::NotImplemented {
                    description: format!("Action '{action}' is not implemented"),
                    details: json!({}),
                }
            }

            fn from_wire(code: &str, description: &str, details: Value) -> Self {
                match code {
                    $($code => OCPP1_6Error::$variant { description: description.to_string(), details }),+,
                    _ => OCPP1_6Error::GenericError { description: description.to_string(), details },
                }
            }
        }
    };
}

define_error! {
    NotImplemented => "NotImplemented",
    NotSupported => "NotSupported",
    InternalError => "InternalError",
    ProtocolError => "ProtocolError",
    SecurityError => "SecurityError",
    FormationViolation => "FormationViolation",
    PropertyConstraintViolation => "PropertyConstraintViolation",
    OccurenceConstraintViolation => "OccurenceConstraintViolation",
    TypeConstraintViolation => "TypeConstraintViolation",
    GenericError => "GenericError",
}

impl fmt::Display for OCPP1_6Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.description())
    }
}

impl std::error::Error for OCPP1_6Error {}
