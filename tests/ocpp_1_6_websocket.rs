//! Real WebSocket round-trip: a tokio-tungstenite server accepts one connection and speaks
//! raw OCPP-J, `connect_1_6` drives the real client against it end to end.
#![allow(clippy::result_large_err)]
use futures::{SinkExt, StreamExt};
use ocpp_client::connect_1_6;
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::HeartbeatRequest;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn heartbeat_round_trips_over_a_real_websocket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_hdr_async(
            tcp,
            |_req: &tokio_tungstenite::tungstenite::handshake::server::Request,
             mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "ocpp1.6".parse().unwrap());
                Ok(response)
            },
        )
        .await
        .unwrap();

        let frame = match ws.next().await.unwrap().unwrap() {
            Message::Text(text) => text.to_string(),
            other => panic!("expected a text frame, got {other:?}"),
        };
        let call: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(call[2], "Heartbeat");
        let message_id = call[1].as_str().unwrap().to_string();

        let response = json!([3, message_id, { "currentTime": "2024-01-01T00:00:00Z" }]);
        ws.send(Message::text(serde_json::to_string(&response).unwrap()))
            .await
            .unwrap();
    });

    let client = connect_1_6(&format!("ws://{addr}"), None).await.unwrap();
    let response = client.send_heartbeat(HeartbeatRequest {}).await.unwrap();
    assert_eq!(
        response.current_time,
        OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap()
    );

    server.await.unwrap();
}
