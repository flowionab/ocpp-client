//! The eleven actions the OCPP 1.6 security whitepaper adds on top of 1.6J, which `ocpp-types`
//! 0.2.0 was the first release to define.
//!
//! One round trip each, over the in-memory transport: the four a charge point initiates go out
//! through `send_*`, and the seven a CSMS initiates come back in through `on_*`. That split is
//! the thing worth covering per action - the dispatch machinery underneath is already covered by
//! `tests/ocpp_1_6_fake_transport.rs`, and `tests/action_coverage.rs` guards the wiring existing
//! at all.
mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_1_6::{OCPP1_6Client, OCPP1_6Error};
use ocpp_client::{
    Client, TokioExecutor, TokioTimer, TransportEvent, TransportSink, TransportStream,
};
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::common::{
    CertificateHashData, CertificateSignedResponseStatus, CertificateType,
    DeleteCertificateResponseStatus, ExtendedTriggerMessageRequestRequestedMessage,
    ExtendedTriggerMessageResponseStatus, Firmware, GetInstalledCertificateIdsResponseStatus,
    GetLogResponseStatus, HashAlgorithm, InstallCertificateResponseStatus, Log,
    LogStatusNotificationRequestStatus, LogType, SignCertificateResponseStatus,
    SignedFirmwareStatusNotificationRequestStatus, SignedUpdateFirmwareResponseStatus,
};
use ocpp_types::v16::{
    CertificateSignedRequest, CertificateSignedResponse, DeleteCertificateRequest,
    DeleteCertificateResponse, ExtendedTriggerMessageRequest, ExtendedTriggerMessageResponse,
    GetInstalledCertificateIdsRequest, GetInstalledCertificateIdsResponse, GetLogRequest,
    GetLogResponse, InstallCertificateRequest, InstallCertificateResponse,
    LogStatusNotificationRequest, LogStatusNotificationResponse, SecurityEventNotificationRequest,
    SecurityEventNotificationResponse, SignCertificateRequest,
    SignedFirmwareStatusNotificationRequest, SignedFirmwareStatusNotificationResponse,
    SignedUpdateFirmwareRequest, SignedUpdateFirmwareResponse,
};
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

/// Reads the CALL the client just sent, checks its action name, and answers it with `payload`.
async fn answer_call(
    peer_sink: &mut common::FakeSink,
    peer_source: &mut common::FakeSource,
    action: &str,
    payload: Value,
) -> Value {
    let frame = recv_frame(peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], action);
    let message_id = frame[1].as_str().unwrap().to_string();
    peer_sink
        .send(serde_json::to_string(&json!([3, message_id, payload])).unwrap())
        .await
        .unwrap();
    frame
}

/// Sends `payload` as a CALL from the peer and returns the CALLRESULT the client answered with.
async fn call_the_client(
    peer_sink: &mut common::FakeSink,
    peer_source: &mut common::FakeSource,
    action: &str,
    payload: Value,
) -> Value {
    peer_sink
        .send(serde_json::to_string(&json!([2, "req-1", action, payload])).unwrap())
        .await
        .unwrap();
    let response = recv_frame(peer_source).await;
    assert_eq!(response[0], 3);
    assert_eq!(response[1], "req-1");
    response
}

// ---------------------------------------------------------------------------
// Charge-point-initiated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sign_certificate_round_trips() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    let call = tokio::spawn(async move {
        client
            .send_sign_certificate(SignCertificateRequest {
                csr: "-----BEGIN CERTIFICATE REQUEST-----".into(),
            })
            .await
    });

    let frame = answer_call(
        &mut peer_sink,
        &mut peer_source,
        "SignCertificate",
        json!({"status": "Accepted"}),
    )
    .await;
    assert_eq!(frame[3]["csr"], "-----BEGIN CERTIFICATE REQUEST-----");

    let response = call.await.unwrap().unwrap();
    assert_eq!(response.status, SignCertificateResponseStatus::Accepted);
}

#[tokio::test]
async fn security_event_notification_round_trips() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    let call = tokio::spawn(async move {
        client
            .send_security_event_notification(SecurityEventNotificationRequest {
                tech_info: None,
                timestamp: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
                r#type: "SettingSystemTime".try_into().unwrap(),
            })
            .await
    });

    let frame = answer_call(
        &mut peer_sink,
        &mut peer_source,
        "SecurityEventNotification",
        json!({}),
    )
    .await;
    assert_eq!(frame[3]["type"], "SettingSystemTime");
    assert_eq!(frame[3]["timestamp"], "2024-01-01T00:00:00Z");

    let _: SecurityEventNotificationResponse = call.await.unwrap().unwrap();
}

#[tokio::test]
async fn log_status_notification_round_trips() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    let call = tokio::spawn(async move {
        client
            .send_log_status_notification(LogStatusNotificationRequest {
                request_id: Some(7),
                status: LogStatusNotificationRequestStatus::Uploaded,
            })
            .await
    });

    let frame = answer_call(
        &mut peer_sink,
        &mut peer_source,
        "LogStatusNotification",
        json!({}),
    )
    .await;
    assert_eq!(frame[3]["status"], "Uploaded");
    assert_eq!(frame[3]["requestId"], 7);

    let _: LogStatusNotificationResponse = call.await.unwrap().unwrap();
}

#[tokio::test]
async fn signed_firmware_status_notification_round_trips() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    let call = tokio::spawn(async move {
        client
            .send_signed_firmware_status_notification(SignedFirmwareStatusNotificationRequest {
                request_id: Some(9),
                status: SignedFirmwareStatusNotificationRequestStatus::InstallVerificationFailed,
            })
            .await
    });

    let frame = answer_call(
        &mut peer_sink,
        &mut peer_source,
        "SignedFirmwareStatusNotification",
        json!({}),
    )
    .await;
    assert_eq!(frame[3]["status"], "InstallVerificationFailed");

    let _: SignedFirmwareStatusNotificationResponse = call.await.unwrap().unwrap();
}

// ---------------------------------------------------------------------------
// CSMS-initiated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn certificate_signed_is_answered() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    client
        .on_certificate_signed(|request: CertificateSignedRequest, _client| async move {
            assert!(request.certificate_chain.starts_with("-----BEGIN"));
            Ok::<_, OCPP1_6Error>(CertificateSignedResponse {
                status: CertificateSignedResponseStatus::Accepted,
            })
        })
        .await;

    let response = call_the_client(
        &mut peer_sink,
        &mut peer_source,
        "CertificateSigned",
        json!({"certificateChain": "-----BEGIN CERTIFICATE-----"}),
    )
    .await;
    assert_eq!(response[2]["status"], "Accepted");
}

#[tokio::test]
async fn delete_certificate_is_answered() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    client
        .on_delete_certificate(|request: DeleteCertificateRequest, _client| async move {
            assert_eq!(
                request.certificate_hash_data.hash_algorithm,
                HashAlgorithm::SHA256
            );
            Ok::<_, OCPP1_6Error>(DeleteCertificateResponse {
                status: DeleteCertificateResponseStatus::NotFound,
            })
        })
        .await;

    let response = call_the_client(
        &mut peer_sink,
        &mut peer_source,
        "DeleteCertificate",
        json!({"certificateHashData": {
            "hashAlgorithm": "SHA256",
            "issuerKeyHash": "aa",
            "issuerNameHash": "bb",
            "serialNumber": "01",
        }}),
    )
    .await;
    assert_eq!(response[2]["status"], "NotFound");
}

#[tokio::test]
async fn extended_trigger_message_is_answered() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    client
        .on_extended_trigger_message(
            |request: ExtendedTriggerMessageRequest, _client| async move {
                assert_eq!(
                    request.requested_message,
                    ExtendedTriggerMessageRequestRequestedMessage::SignChargePointCertificate
                );
                Ok::<_, OCPP1_6Error>(ExtendedTriggerMessageResponse {
                    status: ExtendedTriggerMessageResponseStatus::Accepted,
                })
            },
        )
        .await;

    let response = call_the_client(
        &mut peer_sink,
        &mut peer_source,
        "ExtendedTriggerMessage",
        json!({"requestedMessage": "SignChargePointCertificate"}),
    )
    .await;
    assert_eq!(response[2]["status"], "Accepted");
}

#[tokio::test]
async fn get_installed_certificate_ids_is_answered() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    client
        .on_get_installed_certificate_ids(
            |request: GetInstalledCertificateIdsRequest, _client| async move {
                assert_eq!(
                    request.certificate_type,
                    CertificateType::CentralSystemRootCertificate
                );
                Ok::<_, OCPP1_6Error>(GetInstalledCertificateIdsResponse {
                    certificate_hash_data: Some(vec![CertificateHashData {
                        hash_algorithm: HashAlgorithm::SHA256,
                        issuer_key_hash: "aa".try_into().unwrap(),
                        issuer_name_hash: "bb".try_into().unwrap(),
                        serial_number: "01".try_into().unwrap(),
                    }]),
                    status: GetInstalledCertificateIdsResponseStatus::Accepted,
                })
            },
        )
        .await;

    let response = call_the_client(
        &mut peer_sink,
        &mut peer_source,
        "GetInstalledCertificateIds",
        json!({"certificateType": "CentralSystemRootCertificate"}),
    )
    .await;
    assert_eq!(response[2]["status"], "Accepted");
    assert_eq!(response[2]["certificateHashData"][0]["serialNumber"], "01");
}

#[tokio::test]
async fn get_log_is_answered() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    client
        .on_get_log(|request: GetLogRequest, _client| async move {
            assert_eq!(request.log_type, LogType::SecurityLog);
            // The `Log` struct is where 1.6's security actions meet the new timestamp type.
            assert_eq!(
                request.log.oldest_timestamp,
                Some(OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap())
            );
            Ok::<_, OCPP1_6Error>(GetLogResponse {
                filename: Some("security.log".try_into().unwrap()),
                status: GetLogResponseStatus::Accepted,
            })
        })
        .await;

    let response = call_the_client(
        &mut peer_sink,
        &mut peer_source,
        "GetLog",
        json!({
            "log": {
                "remoteLocation": "https://csms.example/logs",
                "oldestTimestamp": "2024-01-01T00:00:00Z",
            },
            "logType": "SecurityLog",
            "requestId": 3,
        }),
    )
    .await;
    assert_eq!(response[2]["status"], "Accepted");
    assert_eq!(response[2]["filename"], "security.log");
}

#[tokio::test]
async fn install_certificate_is_answered() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    client
        .on_install_certificate(|request: InstallCertificateRequest, _client| async move {
            assert_eq!(
                request.certificate_type,
                CertificateType::ManufacturerRootCertificate
            );
            Ok::<_, OCPP1_6Error>(InstallCertificateResponse {
                status: InstallCertificateResponseStatus::Accepted,
            })
        })
        .await;

    let response = call_the_client(
        &mut peer_sink,
        &mut peer_source,
        "InstallCertificate",
        json!({
            "certificate": "-----BEGIN CERTIFICATE-----",
            "certificateType": "ManufacturerRootCertificate",
        }),
    )
    .await;
    assert_eq!(response[2]["status"], "Accepted");
}

#[tokio::test]
async fn signed_update_firmware_is_answered() {
    let (client, mut peer_sink, mut peer_source) = client_pair();

    client
        .on_signed_update_firmware(|request: SignedUpdateFirmwareRequest, _client| async move {
            assert_eq!(request.request_id, 11);
            assert_eq!(
                request.firmware.retrieve_date_time,
                OcppTimestamp::parse_rfc3339("2024-01-01T12:00:00Z").unwrap()
            );
            Ok::<_, OCPP1_6Error>(SignedUpdateFirmwareResponse {
                status: SignedUpdateFirmwareResponseStatus::InvalidCertificate,
            })
        })
        .await;

    let response = call_the_client(
        &mut peer_sink,
        &mut peer_source,
        "SignedUpdateFirmware",
        json!({
            "firmware": {
                "location": "https://csms.example/fw.bin",
                "retrieveDateTime": "2024-01-01T12:00:00Z",
                "signature": "sig",
                "signingCertificate": "-----BEGIN CERTIFICATE-----",
            },
            "requestId": 11,
        }),
    )
    .await;
    assert_eq!(response[2]["status"], "InvalidCertificate");
}

/// The `Firmware`/`Log` structs are constructible from this crate's side too, not just parsed -
/// a station that mirrors a `SignedUpdateFirmware` back into its own state needs that.
#[test]
fn the_new_common_types_are_constructible() {
    let firmware = Firmware {
        install_date_time: None,
        location: "https://csms.example/fw.bin".try_into().unwrap(),
        retrieve_date_time: OcppTimestamp::parse_rfc3339("2024-01-01T12:00:00Z").unwrap(),
        signature: "sig".into(),
        signing_certificate: "-----BEGIN CERTIFICATE-----".into(),
    };
    assert_eq!(firmware.retrieve_date_time.unix_seconds(), 1_704_110_400);

    let log = Log {
        latest_timestamp: None,
        oldest_timestamp: None,
        remote_location: "https://csms.example/logs".try_into().unwrap(),
    };
    assert_eq!(log.remote_location.as_str(), "https://csms.example/logs");
}
