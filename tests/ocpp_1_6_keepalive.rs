//! Ping/pong and scheduled-keepalive behavior, over the in-memory fake transport.
//!
//! Version-independent - all of this lives in the generic `Client<E>` engine - so it is only
//! mirrored under 1.6 rather than duplicated per version, same rationale as the real-transport
//! tests being one-per-version-plus-transport.
//!
//! Intervals here are milliseconds rather than the production 60 seconds, and assertions use
//! generous upper bounds (`timeout(...)` around a channel read) instead of measuring elapsed
//! time, so they stay honest under CI load without needing a mock clock.

mod common;

use common::{PongBehavior, fake_transport_pair, spawn_peer};
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::{
    Client, ClientConfig, ClientError, KeepaliveBehavior, KeepalivePolicy, Reconnector,
    TokioExecutor, TokioTimer, TransportError, TransportEvent, TransportSink, TransportStream,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Long enough that a correct implementation always makes it, short enough that a hung test
/// fails fast rather than hitting the harness timeout.
const GENEROUS: Duration = Duration::from_secs(5);

fn client_with(config: ClientConfig, behavior: PongBehavior) -> (OCPP1_6Client, PingLog) {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let observed = spawn_peer(peer_sink, peer_source, behavior);
    let client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        config,
    );
    (client, observed)
}

type PingLog = tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>;

// ---------------------------------------------------------------------------
// Baseline: the manual one-shot ping. Previously untested entirely.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_ping_resolves_when_the_peer_echoes_the_pong() {
    let (client, _pings) = client_with(
        ClientConfig::new(Duration::from_secs(1)),
        PongBehavior::Echo,
    );

    timeout(GENEROUS, client.send_ping())
        .await
        .expect("send_ping should not hang")
        .expect("an echoed pong should resolve the ping");
}

#[tokio::test]
async fn send_ping_times_out_when_the_peer_never_answers() {
    let (client, _pings) = client_with(
        ClientConfig::new(Duration::from_millis(100)),
        PongBehavior::Silent,
    );

    let result = timeout(GENEROUS, client.send_ping())
        .await
        .expect("send_ping should give up on its own, not hang");

    assert!(matches!(result, Err(ClientError::Timeout)));
}

#[tokio::test]
async fn send_ping_carries_a_correlation_token_as_its_payload() {
    let (client, mut pings) = client_with(
        ClientConfig::new(Duration::from_secs(1)),
        PongBehavior::Echo,
    );

    client.send_ping().await.expect("ping should be answered");
    let payload = timeout(GENEROUS, pings.recv())
        .await
        .expect("a ping should have reached the peer")
        .expect("peer task still running");

    assert_eq!(
        payload.len(),
        8,
        "the ping payload should be the 8-byte correlation token, got {payload:?}"
    );
}

#[tokio::test]
async fn an_inbound_ping_is_answered_with_a_pong_echoing_its_payload() {
    let ((client_sink, client_source), (peer_sink, mut peer_source)) = fake_transport_pair();
    let _client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(1),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    peer_sink.send_event(TransportEvent::Ping(vec![7, 8, 9]));

    let event = timeout(GENEROUS, peer_source.recv_event())
        .await
        .expect("the client should answer an inbound ping")
        .expect("peer end still open");

    match event {
        TransportEvent::Pong(payload) => assert_eq!(
            payload,
            vec![7, 8, 9],
            "RFC 6455 requires the pong to echo the ping's payload"
        ),
        other => panic!("expected a Pong, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Regressions: the pong-matching bugs a scheduled keepalive would have hit.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_timed_out_ping_does_not_poison_later_pings() {
    // Under the old positional matching this was a permanent failure: the timed-out ping left
    // its waiter queued, so every subsequent pong resolved a corpse and every subsequent ping
    // timed out in turn.
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let answer = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // A peer that ignores the first ping and echoes every one after it.
    let answering = answer.clone();
    let mut source = peer_source;
    let sink = peer_sink;
    tokio::spawn(async move {
        while let Some(event) = source.recv_event().await {
            if let TransportEvent::Ping(payload) = event
                && answering.load(Ordering::SeqCst)
            {
                sink.send_event(TransportEvent::Pong(payload));
            }
        }
    });

    let client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_millis(100),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    assert!(
        matches!(client.send_ping().await, Err(ClientError::Timeout)),
        "first ping is deliberately unanswered"
    );

    answer.store(true, Ordering::SeqCst);

    timeout(GENEROUS, client.send_ping())
        .await
        .expect("second ping should not hang")
        .expect("second ping should be answered despite the first having timed out");
}

#[tokio::test]
async fn an_unsolicited_pong_does_not_satisfy_an_in_flight_ping() {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    // Peer never answers pings, but does send one unsolicited pong.
    let _observed = spawn_peer(peer_sink.duplicate(), peer_source, PongBehavior::Silent);

    let client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_millis(300),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    // Eight bytes, so it exercises the token-parsing path, but a value the monotonic counter
    // would need 2^64 pings to reach - unlike `vec![0; 8]`, which is genuinely the first ping's
    // token and would make this test pass for the wrong reason.
    peer_sink.send_event(TransportEvent::Pong(vec![0xff; 8]));

    let result = timeout(GENEROUS, client.send_ping())
        .await
        .expect("send_ping should not hang");

    assert!(
        matches!(result, Err(ClientError::Timeout)),
        "an unsolicited pong must not be mistaken for the answer to our ping"
    );
}

#[tokio::test]
async fn a_pong_echoing_the_wrong_payload_does_not_resolve_the_ping() {
    let (client, _pings) = client_with(
        ClientConfig::new(Duration::from_millis(300)),
        PongBehavior::WrongPayload,
    );

    let result = timeout(GENEROUS, client.send_ping())
        .await
        .expect("send_ping should not hang");

    assert!(
        matches!(result, Err(ClientError::Timeout)),
        "correlation is by payload, so a non-echoing peer's pong must not count"
    );
}

// ---------------------------------------------------------------------------
// The scheduled keepalive loop.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn keepalive_pings_repeatedly_on_the_configured_interval() {
    let config = ClientConfig::new(Duration::from_secs(1)).with_keepalive(
        KeepaliveBehavior::Enabled(KeepalivePolicy::every(Duration::from_millis(20))),
    );
    let (_client, mut pings) = client_with(config, PongBehavior::Echo);

    for nth in 0..3 {
        timeout(GENEROUS, pings.recv())
            .await
            .unwrap_or_else(|_| panic!("keepalive ping {nth} never arrived"))
            .expect("peer task still running");
    }
}

#[tokio::test]
async fn keepalive_tokens_differ_between_pings() {
    let config = ClientConfig::new(Duration::from_secs(1)).with_keepalive(
        KeepaliveBehavior::Enabled(KeepalivePolicy::every(Duration::from_millis(20))),
    );
    let (_client, mut pings) = client_with(config, PongBehavior::Echo);

    let first = timeout(GENEROUS, pings.recv()).await.unwrap().unwrap();
    let second = timeout(GENEROUS, pings.recv()).await.unwrap().unwrap();

    assert_ne!(
        first, second,
        "each ping needs its own token, or two in flight at once could not be told apart"
    );
}

#[tokio::test]
async fn keepalive_is_off_by_default_on_the_bare_constructor() {
    let (_client, mut pings) = client_with(
        ClientConfig::new(Duration::from_millis(50)),
        PongBehavior::Echo,
    );

    let result = timeout(Duration::from_millis(300), pings.recv()).await;
    assert!(
        result.is_err(),
        "from_transport should not put background traffic on a caller-supplied transport"
    );
}

#[tokio::test]
async fn a_zero_interval_disables_pinging() {
    let config = ClientConfig::new(Duration::from_secs(1)).with_keepalive(
        KeepaliveBehavior::Enabled(KeepalivePolicy::every(Duration::ZERO)),
    );
    let (client, mut pings) = client_with(config, PongBehavior::Echo);

    assert_eq!(
        client.ping_interval(),
        None,
        "OCPP reads a WebSocketPingInterval of 0 as disabled"
    );
    assert!(
        timeout(Duration::from_millis(300), pings.recv())
            .await
            .is_err(),
        "a zero interval must not ping"
    );
}

// ---------------------------------------------------------------------------
// Runtime reconfiguration - the WebSocketPingInterval read/write path.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ping_interval_reports_the_configured_value() {
    let config = ClientConfig::new(Duration::from_secs(1)).with_keepalive(
        KeepaliveBehavior::Enabled(KeepalivePolicy::every(Duration::from_secs(90))),
    );
    let (client, _pings) = client_with(config, PongBehavior::Echo);

    assert_eq!(client.ping_interval(), Some(Duration::from_secs(90)));
}

#[tokio::test]
async fn set_ping_interval_round_trips_through_the_getter() {
    let (client, _pings) = client_with(
        ClientConfig::new(Duration::from_secs(1)),
        PongBehavior::Echo,
    );

    assert_eq!(client.ping_interval(), None);

    client.set_ping_interval(Some(Duration::from_secs(30)));
    assert_eq!(client.ping_interval(), Some(Duration::from_secs(30)));

    client.set_ping_interval(Some(Duration::ZERO));
    assert_eq!(
        client.ping_interval(),
        None,
        "a CSMS writing 0 should read back as disabled, not as a zero-length interval"
    );

    client.set_ping_interval(Some(Duration::from_secs(30)));
    client.set_ping_interval(None);
    assert_eq!(client.ping_interval(), None);
}

#[tokio::test]
async fn set_ping_interval_can_enable_keepalive_on_a_client_built_without_it() {
    // The SetVariables path: keepalive was off at construction, and a CSMS turns it on later.
    let (client, mut pings) = client_with(
        ClientConfig::new(Duration::from_secs(1)),
        PongBehavior::Echo,
    );

    assert!(
        timeout(Duration::from_millis(200), pings.recv())
            .await
            .is_err(),
        "no pings before it is enabled"
    );

    client.set_ping_interval(Some(Duration::from_millis(20)));

    timeout(GENEROUS, pings.recv())
        .await
        .expect("enabling keepalive at runtime should start pinging")
        .expect("peer task still running");
}

#[tokio::test]
async fn shortening_the_interval_takes_effect_without_waiting_out_the_old_one() {
    // Starts on an interval far longer than the test's patience; if the loop only re-read the
    // interval after finishing its current sleep, no ping would arrive for an hour.
    let config = ClientConfig::new(Duration::from_secs(1)).with_keepalive(
        KeepaliveBehavior::Enabled(KeepalivePolicy::every(Duration::from_secs(3600))),
    );
    let (client, mut pings) = client_with(config, PongBehavior::Echo);

    client.set_ping_interval(Some(Duration::from_millis(20)));

    timeout(GENEROUS, pings.recv())
        .await
        .expect("set_ping_interval should wake the keepalive task immediately")
        .expect("peer task still running");
}

#[tokio::test]
async fn disabling_at_runtime_stops_the_pings() {
    let config = ClientConfig::new(Duration::from_secs(1)).with_keepalive(
        KeepaliveBehavior::Enabled(KeepalivePolicy::every(Duration::from_millis(20))),
    );
    let (client, mut pings) = client_with(config, PongBehavior::Echo);

    timeout(GENEROUS, pings.recv())
        .await
        .expect("should be pinging to start with")
        .expect("peer task still running");

    client.set_ping_interval(None);

    // Drain anything already in flight before the disable landed, then require silence.
    tokio::time::sleep(Duration::from_millis(100)).await;
    while pings.try_recv().is_ok() {}

    assert!(
        timeout(Duration::from_millis(300), pings.recv())
            .await
            .is_err(),
        "disabling keepalive should stop the pings"
    );
}

// ---------------------------------------------------------------------------
// Dead-peer detection: the point of the whole exercise.
// ---------------------------------------------------------------------------

/// Hands out a fresh loopback transport on every redial, counting the redials. The peer end of
/// each new connection answers pings, so the client settles down after reconnecting.
struct CountingReconnector {
    calls: Arc<AtomicUsize>,
    // Kept alive so the reconnected peer task isn't dropped mid-test.
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

#[tokio::test]
async fn a_peer_that_stops_answering_pings_gets_the_connection_redialled() {
    let calls = Arc::new(AtomicUsize::new(0));
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    // The initial peer is silent: writes succeed, nothing ever comes back. This is the half-open
    // connection that `recv` alone can never notice.
    let _observed = spawn_peer(peer_sink, peer_source, PongBehavior::Silent);

    let config = ClientConfig::new(Duration::from_millis(50))
        .with_reconnect(
            Box::new(CountingReconnector {
                calls: calls.clone(),
                keep: Mutex::new(Vec::new()),
            }),
            ocpp_client::ReconnectPolicy::default(),
        )
        .with_keepalive(KeepaliveBehavior::Enabled(KeepalivePolicy {
            interval: Duration::from_millis(20),
            timeout: Some(Duration::from_millis(50)),
            max_missed: 2,
        }));

    let _client: OCPP1_6Client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        config,
    );

    let deadline = tokio::time::Instant::now() + GENEROUS;
    while calls.load(Ordering::SeqCst) == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "keepalive should have forced a redial after max_missed unanswered pings"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn missed_pings_do_not_redial_before_max_missed_is_reached() {
    let calls = Arc::new(AtomicUsize::new(0));
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let _observed = spawn_peer(peer_sink, peer_source, PongBehavior::Silent);

    // One ping every 10s: within the window below, at most one can be missed, and max_missed=5
    // is far from reached.
    let config = ClientConfig::new(Duration::from_millis(20))
        .with_reconnect(
            Box::new(CountingReconnector {
                calls: calls.clone(),
                keep: Mutex::new(Vec::new()),
            }),
            ocpp_client::ReconnectPolicy::default(),
        )
        .with_keepalive(KeepaliveBehavior::Enabled(KeepalivePolicy {
            interval: Duration::from_secs(10),
            timeout: Some(Duration::from_millis(20)),
            max_missed: 5,
        }));

    let _client: OCPP1_6Client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        config,
    );

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a single missed ping should be tolerated, not cost a reconnect"
    );
}

#[tokio::test]
async fn force_reconnect_is_inert_without_a_reconnector() {
    // With nothing to redial with, honoring the signal would just leave the client deaf - worse
    // than an unanswered ping. So the read loop must keep reading.
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let _observed = spawn_peer(peer_sink, peer_source, PongBehavior::Echo);

    let client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(1),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    client.force_reconnect();
    tokio::time::sleep(Duration::from_millis(100)).await;

    timeout(GENEROUS, client.send_ping())
        .await
        .expect("the read loop should still be running")
        .expect("and still matching pongs");
}
