mod actions;
mod error;

pub use actions::*;
pub use error::OCPP2_0_1Error;

/// OCPP 2.0.1 client - a [`crate::Client`] carrying 2.0.1's error type.
pub type OCPP2_0_1Client = crate::Client<OCPP2_0_1Error>;
