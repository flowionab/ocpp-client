//! Keepalive over a real WebSocket, not the in-memory fake.
//!
//! This exists because the fake transport can't prove the one assumption the whole correlation
//! scheme rests on: that a real peer echoes a ping's payload back in its pong, per RFC 6455
//! §5.5.2-3. The fake echoes because it was written to; `tokio-tungstenite`'s server echoes
//! because the protocol says so. If that ever stopped holding, `send_ping` would time out against
//! every compliant CSMS and keepalive would redial healthy connections - so it is worth one real
//! socket to pin down.
#![allow(clippy::result_large_err)]

use futures::StreamExt;
use ocpp_client::{ConnectOptions, KeepaliveBehavior, KeepalivePolicy, connect_1_6};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

const GENEROUS: Duration = Duration::from_secs(5);

/// Accepts one OCPP 1.6 connection and then just reads, letting tungstenite handle control
/// frames the way any real server does - it answers pings automatically, echoing the payload.
async fn spawn_ocpp_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
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

        // Keep polling so tungstenite's automatic pong replies actually get flushed.
        while let Some(Ok(message)) = ws.next().await {
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
    });

    format!("ws://{addr}")
}

#[tokio::test]
async fn a_real_server_echoes_the_ping_payload_so_send_ping_resolves() {
    let address = spawn_ocpp_server().await;
    let client = connect_1_6(&address, None).await.unwrap();

    timeout(GENEROUS, client.send_ping())
        .await
        .expect("send_ping should not hang against a real server")
        .expect("a compliant server echoes the ping payload, resolving the correlation token");
}

#[tokio::test]
async fn scheduled_keepalive_survives_several_intervals_against_a_real_server() {
    let address = spawn_ocpp_server().await;
    let options = ConnectOptions {
        keepalive: KeepaliveBehavior::Enabled(KeepalivePolicy {
            interval: Duration::from_millis(20),
            timeout: Some(Duration::from_secs(2)),
            max_missed: 2,
        }),
        ..Default::default()
    };

    let client = connect_1_6(&address, Some(options)).await.unwrap();

    // Long enough for many intervals to elapse. If pongs weren't being matched, keepalive would
    // hit max_missed and redial; a redial against this one-shot server leaves the client unable
    // to complete a ping at all, which the assertion below catches.
    tokio::time::sleep(Duration::from_millis(300)).await;

    timeout(GENEROUS, client.send_ping())
        .await
        .expect("the connection should still be up after many keepalive intervals")
        .expect("and pings should still be matching");
}

#[tokio::test]
async fn connect_options_default_to_keepalive_enabled_at_sixty_seconds() {
    let address = spawn_ocpp_server().await;
    let client = connect_1_6(&address, None).await.unwrap();

    assert_eq!(
        client.ping_interval(),
        Some(Duration::from_secs(60)),
        "the WebSocket convenience path should opt into keepalive, and report it for \
         WebSocketPingInterval"
    );
}

#[tokio::test]
async fn keepalive_can_be_disabled_through_connect_options() {
    let address = spawn_ocpp_server().await;
    let options = ConnectOptions {
        keepalive: KeepaliveBehavior::Disabled,
        ..Default::default()
    };

    let client = connect_1_6(&address, Some(options)).await.unwrap();

    assert_eq!(
        client.ping_interval(),
        None,
        "a deployment that forbids unsolicited traffic must be able to turn this off"
    );
}
