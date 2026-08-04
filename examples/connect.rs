//! Connects to an OCPP 1.6 CSMS, answers TriggerMessage/Reset calls, and sends a Heartbeat
//! every 30 seconds until interrupted.
//!
//! Run with: cargo run --example connect -- wss://your-csms.example/CHARGE_POINT_ID
use ocpp_client::connect_1_6;
use ocpp_types::v16::common::{ResetResponseStatus, TriggerMessageResponseStatus};
use ocpp_types::v16::{HeartbeatRequest, ResetResponse, TriggerMessageResponse};
use tokio::signal;
use tokio::time::{Duration, interval};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://127.0.0.1:9000/CP1".to_string());

    let client = connect_1_6(&address, None).await?;

    // Answer CSMS-initiated calls by registering one handler per action.
    client
        .on_trigger_message(|_request, _client| async move {
            Ok(TriggerMessageResponse {
                status: TriggerMessageResponseStatus::Accepted,
            })
        })
        .await;

    client
        .on_reset(|_request, _client| async move {
            Ok(ResetResponse {
                status: ResetResponseStatus::Accepted,
            })
        })
        .await;

    // Send charge-point-initiated calls with the matching send_x method.
    let heartbeat_client = client.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            match heartbeat_client.send_heartbeat(HeartbeatRequest {}).await {
                Ok(response) => println!("heartbeat ok, CSMS time is {}", response.current_time),
                Err(err) => eprintln!("heartbeat failed: {err}"),
            }
        }
    });

    signal::ctrl_c().await?;
    client.disconnect().await?;
    Ok(())
}
