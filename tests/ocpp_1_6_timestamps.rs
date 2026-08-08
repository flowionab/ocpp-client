//! `dateTime` fields, which `ocpp-types` 0.2.0 changed from strings to `OcppTimestamp`.
//!
//! This is a behavioural change, not just a type change: the client now *parses* what the CSMS
//! sends instead of handing the string through untouched, so what a peer may legally write and
//! what happens when it writes something illegal are both this crate's problem now. Version
//! independent - 1.6 carries the cases, exactly like `tests/ocpp_1_6_keepalive.rs`.
mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::{
    Client, ClientError, TokioExecutor, TokioTimer, TransportEvent, TransportSink, TransportStream,
};
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::HeartbeatRequest;
use serde_json::{Value, json};
use std::time::Duration;

fn client_pair() -> (OCPP1_6Client, common::FakeSink, common::FakeSource) {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let client: OCPP1_6Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(5),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );
    (client, peer_sink, peer_source)
}

async fn recv_frame(peer_source: &mut common::FakeSource) -> Value {
    match peer_source.recv().await.unwrap() {
        Some(TransportEvent::Frame(frame)) => serde_json::from_str(&frame).unwrap(),
        other => panic!("expected a frame, got {other:?}"),
    }
}

/// Sends a heartbeat and answers it with `current_time` verbatim, returning whatever the call
/// resolved to.
async fn heartbeat_answered_with(
    current_time: &str,
) -> Result<OcppTimestamp, ClientError<ocpp_client::ocpp_1_6::OCPP1_6Error>> {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    let call = tokio::spawn(async move { client.send_heartbeat(HeartbeatRequest {}).await });

    let frame = recv_frame(&mut peer_source).await;
    let message_id = frame[1].as_str().unwrap().to_string();
    peer_sink
        .send(
            serde_json::to_string(&json!([3, message_id, {"currentTime": current_time}])).unwrap(),
        )
        .await
        .unwrap();

    call.await.unwrap().map(|response| response.current_time)
}

/// OCPP deployments write milliseconds in practice, and the OCPP-J examples do too.
#[tokio::test]
async fn a_timestamp_with_milliseconds_keeps_them() {
    let current_time = heartbeat_answered_with("2013-02-01T20:53:32.486Z")
        .await
        .unwrap();

    assert_eq!(current_time.unix_seconds(), 1_359_752_012);
    assert_eq!(current_time.subsec_nanos(), 486_000_000);
}

/// RFC 3339 allows any fractional precision, not just three digits.
#[tokio::test]
async fn a_timestamp_with_nanosecond_precision_keeps_it() {
    let current_time = heartbeat_answered_with("2024-01-01T00:00:00.123456789Z")
        .await
        .unwrap();

    assert_eq!(current_time.subsec_nanos(), 123_456_789);
}

/// A CSMS in a non-UTC zone is entitled to write its own offset. The instant is what matters for
/// equality, and the offset is kept alongside it for anyone re-rendering the value.
#[tokio::test]
async fn a_non_utc_offset_names_the_same_instant_and_survives() {
    let current_time = heartbeat_answered_with("2024-03-01T14:00:00+02:00")
        .await
        .unwrap();

    assert_eq!(
        current_time,
        OcppTimestamp::parse_rfc3339("2024-03-01T12:00:00Z").unwrap()
    );
    assert_eq!(current_time.utc_offset_minutes(), 120);
}

/// The other side of parsing: a peer that writes something that is not a `dateTime` now fails the
/// call rather than handing a bad string to the caller.
#[tokio::test]
async fn a_malformed_timestamp_fails_the_call_as_a_decode_error() {
    let error = heartbeat_answered_with("yesterday, about noon")
        .await
        .expect_err("a non-RFC-3339 currentTime cannot be decoded");

    assert!(
        matches!(error, ClientError::Decode(_)),
        "expected a decode error, got {error:?}"
    );
}

/// Same for a well-shaped string that names no real instant.
#[tokio::test]
async fn an_impossible_date_fails_the_call_as_a_decode_error() {
    let error = heartbeat_answered_with("2024-02-30T00:00:00Z")
        .await
        .expect_err("February 30th is not an instant");

    assert!(
        matches!(error, ClientError::Decode(_)),
        "expected a decode error, got {error:?}"
    );
}

/// Outbound, the wire form stays what a CSMS expects to read.
#[tokio::test]
async fn an_outgoing_timestamp_is_written_as_rfc_3339() {
    use ocpp_types::v16::StatusNotificationRequest;
    use ocpp_types::v16::common::{ErrorCode, StatusNotificationRequestStatus};

    let (client, mut peer_sink, mut peer_source) = client_pair();

    let call = tokio::spawn(async move {
        client
            .send_status_notification(StatusNotificationRequest {
                connector_id: 1,
                error_code: ErrorCode::NoError,
                info: None,
                status: StatusNotificationRequestStatus::Available,
                timestamp: Some(OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00.250Z").unwrap()),
                vendor_id: None,
                vendor_error_code: None,
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[3]["timestamp"], "2024-01-01T00:00:00.250Z");

    let message_id = frame[1].as_str().unwrap().to_string();
    peer_sink
        .send(serde_json::to_string(&json!([3, message_id, {}])).unwrap())
        .await
        .unwrap();
    call.await.unwrap().unwrap();
}

/// The `chrono` feature is this crate's only reason to forward anything to `ocpp-types`' own, so
/// prove the forwarding reaches the conversions rather than just compiling.
#[cfg(feature = "chrono")]
#[test]
fn the_chrono_feature_converts_both_ways() {
    use chrono::{DateTime, Utc};

    let now = Utc::now();
    let timestamp = OcppTimestamp::from(now);

    assert_eq!(timestamp.unix_seconds(), now.timestamp());
    assert_eq!(DateTime::<Utc>::from(timestamp), now);
}
