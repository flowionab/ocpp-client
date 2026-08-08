//! 2.x's `customData` extension point, which `ocpp-types` 0.2.0 turned into a type parameter on
//! every message type.
//!
//! The generated `send_*`/`on_*`/`wait_for_*` methods pin that parameter to the specification's
//! own `CustomData` (a bare `vendorId`), because a generic method would break inference at every
//! call site that builds a request inline with `custom_data: None` - a defaulted type parameter
//! does not participate in inference. A deployment with a richer vendor extension therefore goes
//! through the generic `Client::call`/`Client::on` with the marker type instantiated itself,
//! which is what this file covers. Version-independent, so 2.0.1 carries the cases and 2.1 gets
//! one to prove the second macro expands the same way.
mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_2_0_1::{OCPP2_0_1Client, OCPP2_0_1Error};
use ocpp_client::ocpp_2_1::OCPP2_1Client;
use ocpp_client::{
    Action, Client, TokioExecutor, TokioTimer, TransportEvent, TransportSink, TransportStream,
};
use ocpp_types::v201::common::{CustomData, ResetEnum, ResetStatusEnum};
use ocpp_types::v201::{HeartbeatRequest, HeartbeatResponse, ResetRequest, ResetResponse};
use ocpp_types::{NoCustomData, OcppTimestamp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// A vendor extension richer than the specification's `vendorId`-only shape - the thing that is
/// impossible to express through the concrete `send_*` methods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct AcmeExtension {
    #[serde(rename = "vendorId")]
    vendor_id: String,
    #[serde(rename = "siteId")]
    site_id: u32,
}

fn client_pair(timeout: Duration) -> (OCPP2_0_1Client, common::FakeSink, common::FakeSource) {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let client: OCPP2_0_1Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        timeout,
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

#[tokio::test]
async fn call_sends_and_reads_back_a_consumers_own_custom_data_shape() {
    use ocpp_client::ocpp_2_0_1::Heartbeat;

    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .call::<Heartbeat<AcmeExtension>>(HeartbeatRequest {
                custom_data: Some(AcmeExtension {
                    vendor_id: "com.acme".into(),
                    site_id: 42,
                }),
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[2], "Heartbeat");
    // The whole point: a field the specification's `CustomData` has no room for reaches the wire.
    assert_eq!(frame[3]["customData"]["siteId"], 42);
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: HeartbeatResponse<AcmeExtension> = HeartbeatResponse {
        current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
        custom_data: Some(AcmeExtension {
            vendor_id: "com.acme".into(),
            site_id: 7,
        }),
    };
    peer_sink
        .send(serde_json::to_string(&json!([3, message_id, response])).unwrap())
        .await
        .unwrap();

    let response = call.await.unwrap().unwrap();
    assert_eq!(response.custom_data.unwrap().site_id, 7);
}

#[tokio::test]
async fn on_answers_with_a_consumers_own_custom_data_shape() {
    use ocpp_client::ocpp_2_0_1::Reset;

    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    client
        .on::<Reset<AcmeExtension>, _, _>(|request, _client| async move {
            assert_eq!(request.custom_data.unwrap().site_id, 1);
            Ok::<_, OCPP2_0_1Error>(ResetResponse {
                custom_data: Some(AcmeExtension {
                    vendor_id: "com.acme".into(),
                    site_id: 2,
                }),
                status: ResetStatusEnum::Accepted,
                status_info: None,
            })
        })
        .await;

    let request = json!({
        "customData": {"vendorId": "com.acme", "siteId": 1},
        "type": "Immediate",
    });
    peer_sink
        .send(serde_json::to_string(&json!([2, "req-1", "Reset", request])).unwrap())
        .await
        .unwrap();

    let response = recv_frame(&mut peer_source).await;
    assert_eq!(response[0], 3);
    assert_eq!(response[2]["customData"]["siteId"], 2);
}

/// The marker's default is this crate's `CustomData`, *not* `ocpp-types`' own `NoCustomData`, so
/// the turbofish-free spelling keeps the vendor id it kept before 0.4.0 rather than silently
/// discarding it.
#[tokio::test]
async fn the_default_marker_keeps_the_specifications_custom_data() {
    use ocpp_client::ocpp_2_0_1::Reset;

    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .call::<Reset>(ResetRequest {
                custom_data: Some(CustomData {
                    vendor_id: "com.acme".try_into().unwrap(),
                }),
                r#type: ResetEnum::Immediate,
                evse_id: None,
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[3]["customData"]["vendorId"], "com.acme");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response = json!({"status": "Accepted", "customData": {"vendorId": "com.acme"}});
    peer_sink
        .send(serde_json::to_string(&json!([3, message_id, response])).unwrap())
        .await
        .unwrap();

    let response = call.await.unwrap().unwrap();
    assert_eq!(response.custom_data.unwrap().vendor_id.as_str(), "com.acme");
}

/// `NoCustomData` is the other end of the trade: it accepts whatever the peer sends and discards
/// it, for a deployment that would rather not carry the field's width at every node.
#[tokio::test]
async fn no_custom_data_accepts_and_discards_what_the_peer_sends() {
    use ocpp_client::ocpp_2_0_1::Reset;

    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    client
        .on::<Reset<NoCustomData>, _, _>(|request, _client| async move {
            // Present, but nothing to read out of it.
            assert!(request.custom_data.is_some());
            Ok::<_, OCPP2_0_1Error>(ResetResponse {
                custom_data: None,
                status: ResetStatusEnum::Accepted,
                status_info: None,
            })
        })
        .await;

    let request = json!({
        "customData": {"vendorId": "com.acme", "siteId": 1},
        "type": "Immediate",
    });
    peer_sink
        .send(serde_json::to_string(&json!([2, "req-1", "Reset", request])).unwrap())
        .await
        .unwrap();

    let response = recv_frame(&mut peer_source).await;
    assert_eq!(response[0], 3);
    assert_eq!(response[2]["status"], "Accepted");
}

/// `ocpp_2_1_action!` expands to the same shape, marker default included.
#[tokio::test]
async fn the_2_1_macro_takes_the_parameter_too() {
    use ocpp_client::ocpp_2_1::Heartbeat;
    use ocpp_types::v21::HeartbeatRequest as HeartbeatRequest2_1;
    use ocpp_types::v21::HeartbeatResponse as HeartbeatResponse2_1;

    let ((client_sink, client_source), (mut peer_sink, mut peer_source)) = fake_transport_pair();
    let client: OCPP2_1Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(5),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    let call = tokio::spawn(async move {
        client
            .call::<Heartbeat<AcmeExtension>>(HeartbeatRequest2_1 {
                custom_data: Some(AcmeExtension {
                    vendor_id: "com.acme".into(),
                    site_id: 42,
                }),
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[3]["customData"]["siteId"], 42);
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: HeartbeatResponse2_1<AcmeExtension> = HeartbeatResponse2_1 {
        current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
        custom_data: None,
    };
    peer_sink
        .send(serde_json::to_string(&json!([3, message_id, response])).unwrap())
        .await
        .unwrap();

    call.await.unwrap().unwrap();
}

/// The marker types are type-level tags: `Action` requires them to be `Send + Sync + 'static`,
/// and the `PhantomData<fn() -> C>` in the macro is what keeps that true independently of `C`.
#[test]
fn a_marker_is_send_and_sync_whatever_the_custom_data_type_is() {
    use ocpp_client::ocpp_2_0_1::Reset;

    fn assert_action<A: Action + Send + Sync + 'static>() {}

    // Not `Sync`, and deliberately so - the marker must not inherit it.
    struct NotSync(#[allow(dead_code)] std::cell::Cell<u32>);

    impl Serialize for NotSync {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_unit()
        }
    }

    impl<'de> Deserialize<'de> for NotSync {
        fn deserialize<D: serde::Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
            Ok(NotSync(std::cell::Cell::new(0)))
        }
    }

    assert_action::<Reset>();
    assert_action::<Reset<NoCustomData>>();
    assert_action::<Reset<AcmeExtension>>();
    // `Reset<NotSync>` is still `Send + Sync` as a *marker*; it just isn't a usable `Action`,
    // since `Action`'s associated types carry the bounds the payload needs.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Reset<NotSync>>();
}
