use crate::ocpp_1_6_client::OCPP1_6Client;

pub enum Client {
    OCPP1_6(OCPP1_6Client),
}