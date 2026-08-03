use crate::error::ProtocolError;
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use serde_json::{Value, json};

macro_rules! define_error {
    ($($variant:ident => $code:literal),+ $(,)?) => {
        /// An OCPP 2.1 CALLERROR, one variant per error code defined by the spec. The OCPP-J
        /// RPC framework error codes are unchanged between 2.0.1 and 2.1, so this mirrors
        /// `OCPP2_0_1Error` exactly.
        #[derive(Debug, Clone)]
        pub enum OCPP2_1Error {
            $(
                #[doc = $code]
                $variant { description: String, details: Value }
            ),+
        }

        impl ProtocolError for OCPP2_1Error {
            fn code(&self) -> &str {
                match self {
                    $(OCPP2_1Error::$variant { .. } => $code),+
                }
            }

            fn description(&self) -> &str {
                match self {
                    $(OCPP2_1Error::$variant { description, .. } => description),+
                }
            }

            fn details(&self) -> &Value {
                match self {
                    $(OCPP2_1Error::$variant { details, .. } => details),+
                }
            }

            fn not_implemented(action: &str) -> Self {
                OCPP2_1Error::NotImplemented {
                    description: format!("Action '{action}' is not implemented"),
                    details: json!({}),
                }
            }

            fn from_wire(code: &str, description: &str, details: Value) -> Self {
                match code {
                    $($code => OCPP2_1Error::$variant { description: description.to_string(), details }),+,
                    _ => OCPP2_1Error::GenericError { description: description.to_string(), details },
                }
            }
        }
    };
}

define_error! {
    FormatViolation => "FormatViolation",
    GenericError => "GenericError",
    InternalError => "InternalError",
    MessageTypeNotSupported => "MessageTypeNotSupported",
    NotImplemented => "NotImplemented",
    NotSupported => "NotSupported",
    OccurrenceConstraintViolation => "OccurrenceConstraintViolation",
    PropertyConstraintViolation => "PropertyConstraintViolation",
    ProtocolError => "ProtocolError",
    RpcFrameworkError => "RpcFrameworkError",
    SecurityError => "SecurityError",
    TypeConstraintViolation => "TypeConstraintViolation",
}

impl fmt::Display for OCPP2_1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.description())
    }
}

impl core::error::Error for OCPP2_1Error {}
