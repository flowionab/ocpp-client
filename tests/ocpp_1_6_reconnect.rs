//! Real WebSocket reconnect: the server accepts a connection, answers one Heartbeat, then
//! closes the socket. `connect_1_6`'s default `ReconnectBehavior` should notice the drop and
//! redial automatically, so a second Heartbeat sent afterwards still round-trips - over a new
//! TCP connection the client established on its own.
#![allow(clippy::result_large_err)]
use futures::{SinkExt, StreamExt};
use ocpp_client::{ConnectOptions, ReconnectBehavior, ReconnectPolicy, connect_1_6};
use ocpp_types::v16::HeartbeatRequest;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn client_reconnects_after_the_connection_drops() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        for attempt in 0..2 {
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

            if attempt == 0 {
                // Drop the connection so the client has to reconnect for round two.
                ws.close(None).await.unwrap();
            }
        }
    });

    let options = ConnectOptions {
        reconnect: ReconnectBehavior::Enabled(ReconnectPolicy {
            initial_delay: Duration::from_millis(20),
            max_delay: Duration::from_millis(100),
            multiplier: 2,
        }),
        ..Default::default()
    };
    let client = connect_1_6(&format!("ws://{addr}"), Some(options))
        .await
        .unwrap();

    let first = client.send_heartbeat(HeartbeatRequest {}).await.unwrap();
    assert_eq!(first.current_time, "2024-01-01T00:00:00Z");

    // Give the background read loop time to notice the close and redial before we send again.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let second = client.send_heartbeat(HeartbeatRequest {}).await.unwrap();
    assert_eq!(second.current_time, "2024-01-01T00:00:00Z");

    server.await.unwrap();
}
