//! The client's internal tables must not grow without bound, and superseded handler tasks must
//! actually stop.
//!
//! None of this is observable from ordinary use - a leaked map entry or a task looping forever on
//! a channel nobody writes to shows up only as memory growth on a charge point that has been up
//! for weeks. So these use the `test`-feature accessors and a drop-detecting callback.
#![cfg(feature = "test")]

mod common;

use common::{PongBehavior, fake_transport_pair, spawn_peer};
use ocpp_client::ocpp_1_6::{Heartbeat, OCPP1_6Client};
use ocpp_client::{Client, ClientConfig, ClientError, TokioExecutor, TokioTimer, TransportEvent};
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::{HeartbeatRequest, HeartbeatResponse};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

const GENEROUS: Duration = Duration::from_secs(5);

/// A client whose peer answers pings but never answers CALLs, so every request times out.
fn client_that_never_gets_answers() -> OCPP1_6Client {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    // Leaked on purpose: the peer must stay alive for the client's whole life.
    Box::leak(Box::new(spawn_peer(
        peer_sink,
        peer_source,
        PongBehavior::Echo,
    )));
    Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        ClientConfig::new(Duration::from_millis(50)),
    )
}

// ---------------------------------------------------------------------------
// 1. pending_responses
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_timed_out_request_does_not_leak_its_pending_entry() {
    let client = client_that_never_gets_answers();
    assert_eq!(client.pending_request_count().await, 0);

    let result = client.send_heartbeat(HeartbeatRequest {}).await;
    assert!(matches!(result, Err(ClientError::Timeout)));

    assert_eq!(
        client.pending_request_count().await,
        0,
        "a request that gave up must remove its own waiter, or the table grows forever"
    );
}

#[tokio::test]
async fn many_timed_out_requests_do_not_accumulate() {
    let client = client_that_never_gets_answers();

    for _ in 0..25 {
        let _ = client.send_heartbeat(HeartbeatRequest {}).await;
    }

    assert_eq!(
        client.pending_request_count().await,
        0,
        "25 timed-out requests should leave nothing behind"
    );
}

#[tokio::test]
async fn a_request_that_could_not_be_sent_does_not_leak_its_pending_entry() {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    drop(peer_source); // every send on the client's sink now fails
    drop(peer_sink);

    let client: OCPP1_6Client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        ClientConfig::new(Duration::from_millis(50)),
    );

    let result = client.send_heartbeat(HeartbeatRequest {}).await;
    assert!(
        matches!(result, Err(ClientError::Transport(_))),
        "expected a transport error, got {result:?}"
    );
    assert_eq!(
        client.pending_request_count().await,
        0,
        "a request that never made it onto the wire must clean up after itself too"
    );
}

/// The equivalent guarantee for pings, which was fixed earlier - here so the two tables are
/// covered by the same suite and neither regresses silently.
#[tokio::test]
async fn a_timed_out_ping_does_not_leak_its_waiter() {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    Box::leak(Box::new(spawn_peer(
        peer_sink,
        peer_source,
        PongBehavior::Silent,
    )));
    let client: OCPP1_6Client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        ClientConfig::new(Duration::from_millis(50)),
    );

    assert!(matches!(
        client.send_ping().await,
        Err(ClientError::Timeout)
    ));
    assert_eq!(client.pending_ping_count().await, 0);
}

// ---------------------------------------------------------------------------
// 2. superseded `on()` handlers
// ---------------------------------------------------------------------------

/// Signals on drop, so a test can observe that the task owning it has actually ended.
struct DropSignal(Option<mpsc::UnboundedSender<()>>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn replacing_a_handler_stops_the_previous_handler_task() {
    let client = client_that_never_gets_answers();

    let (dropped_tx, mut dropped) = mpsc::unbounded_channel();
    let guard = DropSignal(Some(dropped_tx));
    client
        .on::<Heartbeat, _, _>(move |_req, _client| {
            // Captured so the guard lives exactly as long as this task's future.
            let _ = &guard;
            async move {
                Ok(HeartbeatResponse {
                    current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
                })
            }
        })
        .await;

    assert!(
        dropped.try_recv().is_err(),
        "the handler task should still be running before it is replaced"
    );

    // Replace it. The old task is now unreachable - nothing can ever deliver to its channel.
    client
        .on::<Heartbeat, _, _>(|_req, _client| async move {
            Ok(HeartbeatResponse {
                current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
            })
        })
        .await;

    timeout(GENEROUS, dropped.recv())
        .await
        .expect("the superseded handler task must exit, not loop forever on a dead channel")
        .expect("drop signal sender still alive");
}

#[tokio::test]
async fn the_replacement_handler_is_the_one_that_answers() {
    let ((client_sink, client_source), (peer_sink, mut peer_source)) = fake_transport_pair();
    let client: OCPP1_6Client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        ClientConfig::new(Duration::from_millis(200)),
    );

    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));

    let counter = first_calls.clone();
    client
        .on::<Heartbeat, _, _>(move |_req, _client| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(HeartbeatResponse {
                    current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
                })
            }
        })
        .await;

    let counter = second_calls.clone();
    client
        .on::<Heartbeat, _, _>(move |_req, _client| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(HeartbeatResponse {
                    current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
                })
            }
        })
        .await;

    peer_sink.send_event(TransportEvent::Frame(
        r#"[2,"m1","Heartbeat",{}]"#.to_string(),
    ));

    timeout(GENEROUS, peer_source.recv_event())
        .await
        .expect("the client should answer the call")
        .expect("peer end still open");

    assert_eq!(second_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        first_calls.load(Ordering::SeqCst),
        0,
        "the replaced handler must not run"
    );
}

// ---------------------------------------------------------------------------
// 3. wait_for leaves the action registered
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wait_for_unregisters_so_later_calls_are_not_swallowed() {
    let ((client_sink, client_source), (peer_sink, mut peer_source)) = fake_transport_pair();
    let client: OCPP1_6Client = Client::from_transport_with_config(
        Box::new(client_sink),
        Box::new(client_source),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        ClientConfig::new(Duration::from_millis(300)),
    );

    let waiting = {
        let client = client.clone();
        tokio::spawn(async move {
            client
                .wait_for::<Heartbeat, _, _>(|_req, _client| async move {
                    Ok(HeartbeatResponse {
                        current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
                    })
                })
                .await
        })
    };

    tokio::time::sleep(Duration::from_millis(50)).await;
    peer_sink.send_event(TransportEvent::Frame(
        r#"[2,"m1","Heartbeat",{}]"#.to_string(),
    ));

    // The CALLRESULT for the awaited call.
    let first = timeout(GENEROUS, peer_source.recv_event())
        .await
        .expect("wait_for should answer the call it was waiting for")
        .expect("peer end still open");
    match first {
        TransportEvent::Frame(frame) => assert!(frame.starts_with("[3,"), "expected a CALLRESULT"),
        other => panic!("expected a frame, got {other:?}"),
    }

    timeout(GENEROUS, waiting)
        .await
        .expect("wait_for should return")
        .expect("task should not panic")
        .expect("wait_for should have parsed the request");

    // A second call for the same action. With the registration left behind, this went into a
    // channel nobody reads and the CSMS got no answer at all - not even an error.
    peer_sink.send_event(TransportEvent::Frame(
        r#"[2,"m2","Heartbeat",{}]"#.to_string(),
    ));

    let second = timeout(Duration::from_secs(2), peer_source.recv_event())
        .await
        .expect("a call with no registered handler must still be answered, not swallowed")
        .expect("peer end still open");

    match second {
        TransportEvent::Frame(frame) => {
            assert!(
                frame.starts_with("[4,"),
                "expected a CALLERROR for an unhandled action, got {frame}"
            );
            assert!(
                frame.contains("NotImplemented"),
                "expected NotImplemented, got {frame}"
            );
        }
        other => panic!("expected a frame, got {other:?}"),
    }
}
