//! Fast tests driving `OCPP1_6Client` over an in-memory transport - no networking involved,
//! so these exercise the CALL/RESULT/ERROR dispatch and timeout logic directly.
mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_1_6::{OCPP1_6Client, OCPP1_6Error};
use ocpp_client::{
    Client, ClientError, ProtocolError, TokioExecutor, TokioTimer, TransportEvent, TransportSink,
    TransportStream,
};
use rust_ocpp::v1_6::messages::heart_beat::{HeartbeatRequest, HeartbeatResponse};
use rust_ocpp::v1_6::messages::trigger_message::{TriggerMessageRequest, TriggerMessageResponse};
use rust_ocpp::v1_6::types::{MessageTrigger, TriggerMessageStatus};
use serde_json::{Value, json};
use std::time::Duration;

fn client_pair(timeout: Duration) -> (OCPP1_6Client, common::FakeSink, common::FakeSource) {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        timeout,
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );
    (client, peer_sink, peer_source)
}

async fn recv_frame(peer_source: &mut common::FakeSource) -> Value {
    match peer_source.recv().await.unwrap() {
        Some(TransportEvent::Frame(frame)) => serde_json::from_str(&frame).unwrap(),
        other => panic!("expected a frame, got {other:?}"),
    }
}

#[tokio::test]
async fn call_resolves_on_matching_result() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move { client.send_heartbeat(HeartbeatRequest {}).await });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "Heartbeat");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response = HeartbeatResponse {
        current_time: chrono::Utc::now(),
    };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    let response = call.await.unwrap().unwrap();
    assert!(response.current_time.timestamp() > 0);
}

#[tokio::test]
async fn call_surfaces_protocol_error() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move { client.send_heartbeat(HeartbeatRequest {}).await });

    let frame = recv_frame(&mut peer_source).await;
    let message_id = frame[1].as_str().unwrap().to_string();

    let error_frame =
        serde_json::to_string(&json!([4, message_id, "InternalError", "boom", {}])).unwrap();
    peer_sink.send(error_frame).await.unwrap();

    let err = call.await.unwrap().unwrap_err();
    match err {
        ClientError::Protocol(e) => {
            assert_eq!(e.code(), "InternalError");
            assert_eq!(e.description(), "boom");
        }
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

#[tokio::test]
async fn call_times_out_without_a_response() {
    let (client, _peer_sink, _peer_source) = client_pair(Duration::from_millis(50));

    let err = client
        .send_heartbeat(HeartbeatRequest {})
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Timeout));
}

#[tokio::test]
async fn on_action_answers_a_call_from_the_peer() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    client
        .on_trigger_message(|_req, _client| async move {
            Ok(TriggerMessageResponse {
                status: TriggerMessageStatus::Accepted,
            })
        })
        .await;

    let request = TriggerMessageRequest {
        requested_message: MessageTrigger::Heartbeat,
        connector_id: None,
    };
    let call_frame =
        serde_json::to_string(&json!([2, "req-1", "TriggerMessage", request])).unwrap();
    peer_sink.send(call_frame).await.unwrap();

    let response = recv_frame(&mut peer_source).await;
    assert_eq!(response[0], 3);
    assert_eq!(response[1], "req-1");
    assert_eq!(response[2]["status"], "Accepted");
}

#[tokio::test]
async fn unregistered_action_gets_not_implemented() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));
    let _keep_alive = client;

    let call_frame =
        serde_json::to_string(&json!([2, "req-2", "Reset", {"type": "Hard"}])).unwrap();
    peer_sink.send(call_frame).await.unwrap();

    let response = recv_frame(&mut peer_source).await;
    assert_eq!(response[0], 4);
    assert_eq!(response[1], "req-2");
    assert_eq!(response[2], OCPP1_6Error::not_implemented("Reset").code());
}
