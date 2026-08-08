//! Reconnect backoff against peers that *accept* the connection and then don't work.
//!
//! The bug these pin down: `ReconnectPolicy` only delayed *failed* dials, and the attempt counter
//! reset on every successful one. A peer that completes the WebSocket handshake and then closes
//! immediately therefore got redialled with no delay whatsoever, unboundedly - measured at ~9,900
//! connections in 2 seconds, about 5k/s from a single charge point. Real triggers are ordinary: a
//! CSMS rejecting the charge point at the application layer, an overloaded endpoint, a load
//! balancer with no live backend.
//!
//! Escalation is now driven by whether a connection carried any inbound traffic, not by whether
//! the dial completed, and the delay is applied before every dial rather than only after a failure.
#![allow(clippy::result_large_err)]

use futures::{SinkExt, StreamExt};
use ocpp_client::{ConnectOptions, ReconnectBehavior, ReconnectPolicy, connect_1_6};
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::HeartbeatRequest;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

type ServerResponse = tokio_tungstenite::tungstenite::handshake::server::Response;
type ServerRequest = tokio_tungstenite::tungstenite::handshake::server::Request;
type ServerErrorResponse = tokio_tungstenite::tungstenite::handshake::server::ErrorResponse;

/// Accepts the handshake, picking `ocpp1.6`. Shared by every server in this file.
fn negotiate_1_6(
    _req: &ServerRequest,
    mut response: ServerResponse,
) -> Result<ServerResponse, ServerErrorResponse> {
    response
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "ocpp1.6".parse().unwrap());
    Ok(response)
}

/// Accepts every connection, completes the WebSocket handshake, then drops it immediately.
/// Counts how many it got.
async fn spawn_accept_then_close_server() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let connections = Arc::new(AtomicUsize::new(0));

    let accepted = connections.clone();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            accepted.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                // Handshake, then drop: the dial "succeeds" and the connection is useless.
                let _ = tokio_tungstenite::accept_hdr_async(tcp, negotiate_1_6).await;
            });
        }
    });

    (format!("ws://{addr}"), connections)
}

#[tokio::test]
async fn an_accept_then_close_peer_does_not_get_hammered() {
    let (address, connections) = spawn_accept_then_close_server().await;

    let options = ConnectOptions {
        reconnect: ReconnectBehavior::Enabled(ReconnectPolicy {
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(400),
            multiplier: 2,
            jitter: true,
        }),
        ..Default::default()
    };
    let _client = connect_1_6(&address, Some(options)).await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    let total = connections.load(Ordering::SeqCst);

    // With 50ms initial delay doubling to a 400ms cap, two seconds allows roughly:
    // 25 + 50 + 100 + 200 + 200*n ms of waiting, so on the order of ten dials. The old behavior
    // produced ~9,900. A generous ceiling still separates the two by three orders of magnitude.
    assert!(
        total < 40,
        "expected the backoff to hold the rate down, got {total} connections in 2s"
    );
    // And it must not have given up either - a charge point retries its CSMS indefinitely.
    assert!(
        total >= 2,
        "expected it to keep retrying, got only {total} connections"
    );
}

#[tokio::test]
async fn backoff_escalates_across_repeated_useless_connections() {
    let (address, connections) = spawn_accept_then_close_server().await;

    let options = ConnectOptions {
        reconnect: ReconnectBehavior::Enabled(ReconnectPolicy {
            initial_delay: Duration::from_millis(40),
            max_delay: Duration::from_secs(30),
            multiplier: 4,
            jitter: false,
        }),
        ..Default::default()
    };
    let _client = connect_1_6(&address, Some(options)).await.unwrap();

    // 40ms, then 160ms, then 640ms, then 2560ms... so within a second the delays have grown past
    // what a non-escalating backoff would ever reach.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let early = connections.load(Ordering::SeqCst);

    tokio::time::sleep(Duration::from_secs(2)).await;
    let late = connections.load(Ordering::SeqCst);

    assert!(
        late - early <= 2,
        "the delay should have escalated well past a second, but {} more dials happened in the \
         following two seconds",
        late - early
    );
}

/// The counterpart guarantee: escalation must not punish a connection that genuinely worked. A
/// connection that carried traffic resets the backoff, so its later drop is redialled on the
/// *initial* delay rather than the escalated one.
///
/// Measured through `on_reconnect` latency rather than by sending a second request, because a
/// request issued before the redial completes would just wait out the request timeout and tell us
/// nothing about the backoff.
#[tokio::test]
async fn a_connection_that_carried_traffic_resets_the_backoff() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        // Two accept-then-close connections (plus `connect_1_6`'s own initial dial) push the
        // backoff up to its third step before anything works.
        for _ in 0..2 {
            let (tcp, _) = listener.accept().await.unwrap();
            let _ = tokio_tungstenite::accept_hdr_async(tcp, negotiate_1_6).await;
        }

        // A connection that answers a Heartbeat - real inbound traffic - and then drops.
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_hdr_async(tcp, negotiate_1_6)
            .await
            .unwrap();
        let frame = match ws.next().await.unwrap().unwrap() {
            Message::Text(text) => text.to_string(),
            other => panic!("expected a text frame, got {other:?}"),
        };
        let call: Value = serde_json::from_str(&frame).unwrap();
        let message_id = call[1].as_str().unwrap().to_string();
        let response = json!([3, message_id, { "currentTime": "2024-01-01T00:00:00Z" }]);
        ws.send(Message::text(serde_json::to_string(&response).unwrap()))
            .await
            .unwrap();
        ws.close(None).await.unwrap();

        // Keep accepting afterwards so the post-traffic redial has something to reach.
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                if let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, negotiate_1_6).await {
                    while let Some(Ok(message)) = ws.next().await {
                        if matches!(message, Message::Close(_)) {
                            break;
                        }
                    }
                }
            });
        }
    });

    // multiplier 8 makes the difference unmistakable: reset means the next delay is 30ms, while
    // continued escalation would put it at 30 * 8^2 = 1920ms.
    let options = ConnectOptions {
        reconnect: ReconnectBehavior::Enabled(ReconnectPolicy {
            initial_delay: Duration::from_millis(30),
            max_delay: Duration::from_secs(10),
            multiplier: 8,
            jitter: false,
        }),
        ..Default::default()
    };
    let client = connect_1_6(&format!("ws://{addr}"), Some(options))
        .await
        .unwrap();

    let (redials_tx, mut redials) = tokio::sync::mpsc::unbounded_channel();
    client
        .on_reconnect(move |_| {
            let tx = redials_tx.clone();
            async move {
                let _ = tx.send(tokio::time::Instant::now());
            }
        })
        .await;

    // Let the client climb through the two useless connections and settle on the working one.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client.send_heartbeat(HeartbeatRequest {}),
    )
    .await
    .expect("should have reached the working connection by now")
    .expect("heartbeat should round-trip");
    assert_eq!(
        response.current_time,
        OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap()
    );
    let traffic_at = tokio::time::Instant::now();

    // The first redial *after* that traffic is the one whose delay we care about.
    let redial_at = loop {
        let at = tokio::time::timeout(Duration::from_secs(5), redials.recv())
            .await
            .expect("the dropped connection should be redialled")
            .expect("on_reconnect sender still alive");
        if at >= traffic_at {
            break at;
        }
    };

    let gap = redial_at - traffic_at;
    assert!(
        gap < Duration::from_millis(800),
        "a connection that carried traffic should reset the backoff, so the redial should take \
         about 30ms; it took {gap:?} (escalation would have made it ~1920ms)"
    );

    server.abort();
}

#[tokio::test]
async fn disconnect_during_a_long_backoff_takes_effect_immediately() {
    let (address, connections) = spawn_accept_then_close_server().await;

    let options = ConnectOptions {
        reconnect: ReconnectBehavior::Enabled(ReconnectPolicy {
            // Long enough that a client waiting it out would blow the assertion below.
            initial_delay: Duration::from_secs(30),
            max_delay: Duration::from_secs(60),
            multiplier: 2,
            jitter: false,
        }),
        ..Default::default()
    };
    let client = connect_1_6(&address, Some(options)).await.unwrap();

    // Let the initial accept-then-close land, putting the read loop into its 30s backoff.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let before = connections.load(Ordering::SeqCst);

    tokio::time::timeout(Duration::from_secs(2), client.disconnect())
        .await
        .expect("disconnect must not block on the backoff")
        .unwrap();

    assert!(client.is_closed());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        connections.load(Ordering::SeqCst),
        before,
        "no dial should happen after an explicit disconnect"
    );
}
