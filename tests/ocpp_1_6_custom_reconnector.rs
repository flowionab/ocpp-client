//! A caller-supplied `Reconnector` decides where a dropped connection is redialled - which is
//! what lets a charge point move to a different CSMS address without tearing down its `Client`
//! (and with it every registered handler, every in-flight request and every queued message).
//!
//! The server here accepts on address A, answers one Heartbeat, then closes. The custom
//! reconnector redials **address B** instead, so the second Heartbeat round-trips against a
//! server the initial `connect_1_6` call never knew about.
#![allow(clippy::result_large_err)]
use futures::{SinkExt, StreamExt};
use ocpp_client::{
    ConnectOptions, OcppVersion, ReconnectBehavior, ReconnectPolicy, Reconnector, TransportError,
    TransportSink, TransportStream, connect_1_6, websocket_transport,
};
use ocpp_types::v16::HeartbeatRequest;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// Redials whatever address it is currently pointed at, rather than the one the connection
/// started on. The `Mutex` is what makes the target swappable while the client is live.
struct SwitchableReconnector {
    address: Arc<Mutex<String>>,
}

impl Reconnector for SwitchableReconnector {
    fn connect<'a>(
        &'a self,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        (Box<dyn TransportSink>, Box<dyn TransportStream>),
                        TransportError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let address = self.address.lock().unwrap().clone();
            websocket_transport(&address, OcppVersion::V1_6, None).await
        })
    }
}

/// Accepts one connection, answers one Heartbeat, then closes unless `keep_open`.
async fn serve_one_heartbeat(listener: TcpListener, keep_open: bool) {
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

    ws.send(Message::text(
        serde_json::to_string(&json!([3, message_id, { "currentTime": "2024-01-01T00:00:00Z" }]))
            .unwrap(),
    ))
    .await
    .unwrap();

    if !keep_open {
        ws.close(None).await.unwrap();
    } else {
        // Hold the socket open so the client does not redial a third time mid-assertion.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tokio::test]
async fn a_custom_reconnector_redials_an_address_the_client_was_never_given() {
    let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_addr = first.local_addr().unwrap();
    let second_addr = second.local_addr().unwrap();

    let first_server = tokio::spawn(serve_one_heartbeat(first, false));
    let second_server = tokio::spawn(serve_one_heartbeat(second, true));

    // Point the reconnector at the *second* address from the outset: after the first server
    // hangs up, that is where the client must go.
    let target = Arc::new(Mutex::new(format!("ws://{second_addr}")));
    let options = ConnectOptions {
        reconnect: ReconnectBehavior::Enabled(ReconnectPolicy {
            initial_delay: Duration::from_millis(20),
            max_delay: Duration::from_millis(100),
            multiplier: 2,
        }),
        reconnector: Some(Arc::new(SwitchableReconnector {
            address: target.clone(),
        })),
        ..Default::default()
    };

    let client = connect_1_6(&format!("ws://{first_addr}"), Some(options))
        .await
        .unwrap();

    assert_eq!(
        client
            .send_heartbeat(HeartbeatRequest {})
            .await
            .unwrap()
            .current_time,
        "2024-01-01T00:00:00Z"
    );
    first_server.await.unwrap();

    // Give the read loop time to notice the close and redial the second address.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Same `client` - so a real caller's handlers, queues and in-flight state all survive the
    // move - but a different server is answering.
    assert_eq!(
        client
            .send_heartbeat(HeartbeatRequest {})
            .await
            .unwrap()
            .current_time,
        "2024-01-01T00:00:00Z"
    );
    second_server.await.unwrap();
}

#[tokio::test]
async fn reconnect_disabled_beats_a_supplied_reconnector() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve_one_heartbeat(listener, false));

    let unreachable = Arc::new(Mutex::new(String::from("ws://127.0.0.1:1")));
    let options = ConnectOptions {
        reconnect: ReconnectBehavior::Disabled,
        reconnector: Some(Arc::new(SwitchableReconnector {
            address: unreachable,
        })),
        ..Default::default()
    };

    let client = connect_1_6(&format!("ws://{addr}"), Some(options))
        .await
        .unwrap();
    assert!(client.send_heartbeat(HeartbeatRequest {}).await.is_ok());
    server.await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    // `Disabled` is an explicit "do not redial", so supplying a reconnector must not quietly
    // turn reconnect back on: the client stays down.
    assert!(
        tokio::time::timeout(
            Duration::from_millis(500),
            client.send_heartbeat(HeartbeatRequest {})
        )
        .await
        .map(|result| result.is_err())
        .unwrap_or(true)
    );
}
