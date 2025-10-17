use rust_ocpp::v1_6::messages::trigger_message::TriggerMessageResponse;
use tokio::signal;
use ocpp_client::{connect_1_6};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = connect_1_6("wss://ocpp.flowion.app/easee/EH123123", None).await?;

    client.on_trigger_message(|_, _| async move {
        Ok(TriggerMessageResponse::default())
    }).await;

    signal::ctrl_c().await?;

    Ok(())
}