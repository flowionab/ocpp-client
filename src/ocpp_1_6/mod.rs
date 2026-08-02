mod actions;
mod error;

pub use actions::*;
pub use error::OCPP1_6Error;

/// OCPP 1.6 client - a [`crate::Client`] carrying 1.6's error type.
pub type OCPP1_6Client = crate::Client<OCPP1_6Error>;
