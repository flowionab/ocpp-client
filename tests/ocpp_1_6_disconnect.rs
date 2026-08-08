//! `disconnect()` must actually disconnect - and stay disconnected.
//!
//! The race this pins down: `disconnect()` closed the sink, the read loop saw the resulting EOF,
//! and (with reconnect enabled, which is the default) treated it as a dropped connection and
//! redialled. So an explicit shutdown got silently undone, and on the default `ConnectOptions` a
//! caller had no way to stop a client at all.
//!
//! The first test uses a real socket, because that is where the bug actually reproduces: the
//! in-memory fake's `close()` doesn't end the peer's stream, so the read loop never saw the EOF
//! that triggered the redial. The rest use the fake transport for the surrounding behavior.
#![allow(clippy::result_large_err)]

mod common;

use common::{PongBehavior, fake_transport_pair, spawn_peer};
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::{
    Client, ClientConfig, ClientError, KeepaliveBehavior, KeepalivePolicy, ReconnectPolicy,
    Reconnector, TokioExecutor, TokioTimer, TransportError, TransportSink, TransportStream,
    connect_1_6,
};
use ocpp_types::v16::HeartbeatRequest;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Longer than `ReconnectPolicy::default()`'s 1s initial backoff, so a client that was going to
/// redial has had time to do it.
const PAST_FIRST_RETRY: Duration = Duration::from_millis(2500);

#[tokio::test]
async fn disconnect_does_not_redial_even_though_reconnect_is_enabled() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let connections = Arc::new(AtomicUsize::new(0));

    let accepted = connections.clone();
    tokio::spawn(async move {
        // Keeps accepting, so a redial would succeed and be counted rather than being refused.
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            accepted.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(
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
                else {
                    return;
                };
                // Hold the connection open. Dropping it here instead would make the server close
                // every connection the instant it opened, so a regression would show up as a hot
                // reconnect loop rather than the single unwanted redial this test is looking for.
                use futures::StreamExt;
                while let Some(Ok(message)) = ws.next().await {
                    if matches!(message, tokio_tungstenite::tungstenite::Message::Close(_)) {
                        break;
                    }
                }
            });
        }
    });

    // `None` means the defaults, which include reconnect enabled - the configuration the race
    // needed.
    let client = connect_1_6(&format!("ws://{addr}"), None).await.unwrap();
    assert_eq!(connections.load(Ordering::SeqCst), 1, "initial connection");

    client.disconnect().await.unwrap();
    tokio::time::sleep(PAST_FIRST_RETRY).await;

    assert_eq!(
        connections.load(Ordering::SeqCst),
        1,
        "disconnect() must not be undone by the reconnector"
    );
}

/// Hands out a fresh loopback transport on every redial, counting them.
struct CountingReconnector {
    calls: Arc<AtomicUsize>,
    keep: Mutex<Vec<tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>>>,
}

impl Reconnector for CountingReconnector {
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
            self.calls.fetch_add(1, Ordering::SeqCst);
            let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
            let observed = spawn_peer(peer_sink, peer_source, PongBehavior::Echo);
            self.keep.lock().await.push(observed);
            Ok((
                Box::new(client_sink) as Box<dyn TransportSink>,
                Box::new(client_source) as Box<dyn TransportStream>,
            ))
        })
    }
}

/// A client with reconnect and fast keepalive, over the fake transport.
fn client_with_everything_on() -> (
    OCPP1_6Client,
    Arc<AtomicUsize>,
    tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let calls = Arc::new(AtomicUsize::new(0));
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let pings = spawn_peer(peer_sink, peer_source, PongBehavior::Echo);

    let config = ClientConfig::new(Duration::from_millis(200))
        .with_reconnect(
            Box::new(CountingReconnector {
                calls: calls.clone(),
                keep: Mutex::new(Vec::new()),
            }),
            ReconnectPolicy::default(),
        )
        .with_keepalive(KeepaliveBehavior::Enabled(KeepalivePolicy::every(
            Duration::from_millis(20),
        )));

    let client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        config,
    );
    (client, calls, pings)
}

#[tokio::test]
async fn disconnect_stops_the_keepalive_pings() {
    let (client, _calls, mut pings) = client_with_everything_on();

    // Confirm it really was pinging first, so the assertion below isn't vacuous.
    tokio::time::timeout(Duration::from_secs(5), pings.recv())
        .await
        .expect("should be pinging before disconnect")
        .expect("peer task still running");

    client.disconnect().await.unwrap();

    // Drain whatever was already in flight when the disconnect landed, then require silence.
    tokio::time::sleep(Duration::from_millis(100)).await;
    while pings.try_recv().is_ok() {}

    assert!(
        tokio::time::timeout(Duration::from_millis(300), pings.recv())
            .await
            .is_err(),
        "a disconnected client must not keep pinging"
    );
}

#[tokio::test]
async fn disconnect_is_idempotent() {
    let (client, calls, _pings) = client_with_everything_on();

    client.disconnect().await.expect("first disconnect");
    client.disconnect().await.expect("second disconnect");
    client.disconnect().await.expect("third disconnect");

    tokio::time::sleep(PAST_FIRST_RETRY).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no redial from any of them"
    );
}

#[tokio::test]
async fn force_reconnect_after_disconnect_does_nothing() {
    let (client, calls, _pings) = client_with_everything_on();

    client.disconnect().await.unwrap();
    client.force_reconnect();

    tokio::time::sleep(PAST_FIRST_RETRY).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "an explicit disconnect outranks a forced reconnect"
    );
}

#[tokio::test]
async fn is_closed_reports_the_explicit_shutdown() {
    let (client, _calls, _pings) = client_with_everything_on();

    assert!(!client.is_closed());
    client.disconnect().await.unwrap();
    assert!(client.is_closed());
}

#[tokio::test]
async fn requests_after_disconnect_fail_fast_as_closed() {
    let (client, _calls, _pings) = client_with_everything_on();
    client.disconnect().await.unwrap();

    // Would otherwise sit until the request timeout waiting for a response that can't arrive.
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        client.send_heartbeat(HeartbeatRequest {}),
    )
    .await
    .expect("should fail immediately, not wait out the timeout");

    assert!(
        matches!(result, Err(ClientError::Closed)),
        "expected ClientError::Closed, got {result:?}"
    );
}

#[tokio::test]
async fn pings_after_disconnect_fail_fast_as_closed() {
    let (client, _calls, _pings) = client_with_everything_on();
    client.disconnect().await.unwrap();

    let result = tokio::time::timeout(Duration::from_millis(100), client.send_ping())
        .await
        .expect("should fail immediately");

    assert!(matches!(result, Err(ClientError::Closed)));
}

#[tokio::test]
async fn set_ping_interval_cannot_revive_a_disconnected_client() {
    let (client, _calls, mut pings) = client_with_everything_on();
    client.disconnect().await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    while pings.try_recv().is_ok() {}

    client.set_ping_interval(Some(Duration::from_millis(20)));

    assert!(
        tokio::time::timeout(Duration::from_millis(300), pings.recv())
            .await
            .is_err(),
        "reconfiguring keepalive must not restart a client the caller shut down"
    );
}

/// The other half of the guarantee: fixing the explicit-disconnect race must not stop a genuine
/// drop from being redialled. (`tests/ocpp_1_6_reconnect.rs` covers this over a real socket; this
/// is the fake-transport version, alongside the disconnect cases it contrasts with.)
#[tokio::test]
async fn an_unrequested_drop_still_redials() {
    let calls = Arc::new(AtomicUsize::new(0));
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    drop(peer_source);

    let config = ClientConfig::new(Duration::from_millis(200)).with_reconnect(
        Box::new(CountingReconnector {
            calls: calls.clone(),
            keep: Mutex::new(Vec::new()),
        }),
        ReconnectPolicy::default(),
    );

    let _client: OCPP1_6Client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        config,
    );

    // Dropping the peer's sink ends the client's inbound stream: an EOF nobody asked for.
    drop(peer_sink);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while calls.load(Ordering::SeqCst) == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "an unrequested EOF should still be redialled"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
