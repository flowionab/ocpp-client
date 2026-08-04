//! Exercises the `wait_for_*` helpers, only compiled when the `test` feature is enabled
//! (`cargo test --features test`).
#![cfg(feature = "test")]

mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::{Client, TokioExecutor, TokioTimer, TransportSink};
use ocpp_types::v16::common::{ResetRequestType, ResetResponseStatus};
use ocpp_types::v16::{ResetRequest, ResetResponse};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn wait_for_reset_returns_the_parsed_request() {
    let ((client_sink, client_source), (mut peer_sink, _peer_source)) = fake_transport_pair();
    let client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(5),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    let call_frame =
        serde_json::to_string(&json!([2, "req-1", "Reset", { "type": "Hard" }])).unwrap();
    peer_sink.send(call_frame).await.unwrap();

    let request = client
        .wait_for_reset(|_req, _client| async move {
            Ok(ResetResponse {
                status: ResetResponseStatus::Accepted,
            })
        })
        .await
        .unwrap();

    assert_eq!(
        request,
        ResetRequest {
            r#type: ResetRequestType::Hard,
        }
    );
}
