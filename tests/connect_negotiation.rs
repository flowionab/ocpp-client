//! Real WebSocket round-trip proving `connect` offers every compiled-in OCPP version and then
//! uses whichever one the server actually picks, instead of requiring the caller to already
//! know the server's version like `connect_1_6`/`connect_2_0_1`/`connect_2_1` do.
#![allow(clippy::result_large_err)]
use futures::{SinkExt, StreamExt};
use ocpp_client::{NegotiatedClient, OcppVersion, connect};
use ocpp_types::v16::HeartbeatRequest as V16HeartbeatRequest;
use ocpp_types::v201::HeartbeatRequest;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn connect_negotiates_whichever_version_the_server_picks() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut offered_protocols = None;
        let mut ws = tokio_tungstenite::accept_hdr_async(
            tcp,
            |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
             mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                offered_protocols = req
                    .headers()
                    .get("Sec-WebSocket-Protocol")
                    .map(|v| v.to_str().unwrap().to_string());
                // Server only understands 2.0.1, even though 2.1 and 1.6 were also offered.
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "ocpp2.0.1".parse().unwrap());
                Ok(response)
            },
        )
        .await
        .unwrap();

        let offered_protocols = offered_protocols.unwrap();
        assert!(offered_protocols.contains("ocpp2.1"));
        assert!(offered_protocols.contains("ocpp2.0.1"));
        assert!(offered_protocols.contains("ocpp1.6"));

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

    let client = connect(&format!("ws://{addr}"), None, None).await.unwrap();
    let client = match client {
        NegotiatedClient::V2_0_1(client) => client,
        _ => panic!("expected the server's ocpp2.0.1 pick, got a different version"),
    };
    let response = client
        .send_heartbeat(HeartbeatRequest { custom_data: None })
        .await
        .unwrap();
    assert_eq!(response.current_time, "2024-01-01T00:00:00Z");

    server.await.unwrap();
}

#[tokio::test]
async fn connect_only_offers_the_caller_supplied_versions() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut offered_protocols = None;
        let mut ws = tokio_tungstenite::accept_hdr_async(
            tcp,
            |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
             mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                offered_protocols = req
                    .headers()
                    .get("Sec-WebSocket-Protocol")
                    .map(|v| v.to_str().unwrap().to_string());
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "ocpp1.6".parse().unwrap());
                Ok(response)
            },
        )
        .await
        .unwrap();

        // Only ocpp1.6 was offered - the caller restricted `versions` to just that one.
        assert_eq!(offered_protocols.unwrap(), "ocpp1.6");

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

    let client = connect(&format!("ws://{addr}"), Some(&[OcppVersion::V1_6]), None)
        .await
        .unwrap();
    let client = match client {
        NegotiatedClient::V1_6(client) => client,
        _ => panic!("expected the server's ocpp1.6 pick, got a different version"),
    };
    let response = client.send_heartbeat(V16HeartbeatRequest {}).await.unwrap();
    assert_eq!(response.current_time, "2024-01-01T00:00:00Z");

    server.await.unwrap();
}
