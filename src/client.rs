use crate::ocpp_1_6::OCPP1_6Client;

/// Enum representing the different OCPP versions that the client can connect to.
#[derive(Clone)]
pub enum Client {
    #[cfg(feature = "ocpp_1_6")]
    OCPP1_6(OCPP1_6Client),
}