use crate::ocpp_1_6::OCPP1_6Client;
use crate::ocpp_2_0_1::OCPP2_0_1Client;

/// Enum representing the different OCPP versions that the client can connect to.
#[derive(Clone)]
pub enum Client {
    #[cfg(feature = "ocpp_1_6")]
    OCPP1_6(OCPP1_6Client),

    #[cfg(feature = "ocpp_2_0_1")]
    OCPP2_0_1(OCPP2_0_1Client),
}