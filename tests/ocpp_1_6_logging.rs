//! `Client`'s background read loop reports malformed input through the `tracing` crate (see
//! CLAUDE.md's "structured logging" follow-up) instead of a bare `eprintln!`. Drives a real
//! `Client` over the in-memory fake transport and asserts a `tracing::warn!` event fires when
//! the peer sends invalid JSON.
mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::{Client, TokioExecutor, TokioTimer, TransportSink};
use std::time::Duration;
use tracing_test::traced_test;

#[tokio::test]
#[traced_test]
async fn malformed_frame_logs_a_warning() {
    let ((client_sink, client_source), (mut peer_sink, _peer_source)) = fake_transport_pair();
    let _client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(5),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    peer_sink.send("not json".to_string()).await.unwrap();

    // Give the background read loop a moment to process the frame and emit the log event.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(logs_contain("ocpp-client: received malformed frame"));
}
