//! Same coverage as tests/ocpp_2_0_1_fake_transport.rs, for OCPP 2.1 - proves the generic
//! `Client<E>` engine works unmodified for a third version's error type.
mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_2_1::{OCPP2_1Client, OCPP2_1Error};
use ocpp_client::{
    Client, ClientError, ProtocolError, TokioExecutor, TokioTimer, TransportEvent, TransportSink,
    TransportStream,
};
use ocpp_types::OcppTimestamp;
use ocpp_types::v21::common::{
    ChargingProfileStatusEnum, ChargingScheduleUpdate, DERControlEnum, DERControlStatusEnum,
    DisplayMessageStatusEnum, MessageContent, MessageFormatEnum, MessageInfo, MessagePriorityEnum,
    MessageTriggerEnum, ResetEnum, ResetStatusEnum, TriggerMessageStatusEnum,
};
use ocpp_types::v21::{
    GetDERControlRequest, GetDERControlResponse, HeartbeatRequest, HeartbeatResponse,
    NotifyPeriodicEventStream, NotifyReportRequest, NotifyReportResponse, ResetRequest,
    ResetResponse, SetDERControlRequest, SetDERControlResponse, SetDisplayMessageRequest,
    SetDisplayMessageResponse, TriggerMessageRequest, TriggerMessageResponse,
    UpdateDynamicScheduleRequest, UpdateDynamicScheduleResponse,
};
use serde_json::{Value, json};
use std::time::Duration;

fn client_pair(timeout: Duration) -> (OCPP2_1Client, common::FakeSink, common::FakeSource) {
    let ((client_sink, client_source), (peer_sink, peer_source)) = fake_transport_pair();
    let client: OCPP2_1Client = Client::from_transport(
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
async fn call_resolves_on_matching_result() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_heartbeat(HeartbeatRequest { custom_data: None })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "Heartbeat");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: HeartbeatResponse = HeartbeatResponse {
        custom_data: None,
        current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
    };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    let response = call.await.unwrap().unwrap();
    assert_eq!(response.current_time.unix_seconds(), 1_704_067_200);
}

#[tokio::test]
async fn call_surfaces_protocol_error() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_heartbeat(HeartbeatRequest { custom_data: None })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    let message_id = frame[1].as_str().unwrap().to_string();

    let error_frame =
        serde_json::to_string(&json!([4, message_id, "InternalError", "boom", {}])).unwrap();
    peer_sink.send(error_frame).await.unwrap();

    let err = call.await.unwrap().unwrap_err();
    match err {
        ClientError::Protocol(e) => {
            assert_eq!(e.code(), "InternalError");
            assert_eq!(e.description(), "boom");
        }
        other => panic!("expected a protocol error, got {other:?}"),
    }
}

#[tokio::test]
async fn call_times_out_without_a_response() {
    let (client, _peer_sink, _peer_source) = client_pair(Duration::from_millis(50));

    let err = client
        .send_heartbeat(HeartbeatRequest { custom_data: None })
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Timeout));
}

#[tokio::test]
async fn on_action_answers_a_call_from_the_peer() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    client
        .on_reset(|_req, _client| async move {
            Ok(ResetResponse {
                custom_data: None,
                status: ResetStatusEnum::Accepted,
                status_info: None,
            })
        })
        .await;

    let request: ResetRequest = ResetRequest {
        custom_data: None,
        r#type: ResetEnum::Immediate,
        evse_id: None,
    };
    let call_frame = serde_json::to_string(&json!([2, "req-1", "Reset", request])).unwrap();
    peer_sink.send(call_frame).await.unwrap();

    let response = recv_frame(&mut peer_source).await;
    assert_eq!(response[0], 3);
    assert_eq!(response[1], "req-1");
    assert_eq!(response[2]["status"], "Accepted");
}

// `NotifyReport` was unimplemented until ocpp-types shipped it (rust-ocpp's `wip_v2_1` module
// was still empty upstream) - see MIGRATION_OCPP_TYPES.md and PRODUCTION_READINESS.md item 4.
#[tokio::test]
async fn call_resolves_notify_report_now_that_its_wired_up() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_notify_report(NotifyReportRequest {
                custom_data: None,
                generated_at: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
                report_data: None,
                request_id: 1,
                seq_no: 0,
                tbc: None,
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "NotifyReport");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: NotifyReportResponse = NotifyReportResponse { custom_data: None };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    call.await.unwrap().unwrap();
}

// TriggerMessage, SetDisplayMessage, GetDERControl, SetDERControl, and UpdateDynamicSchedule all
// had request/response types in ocpp-types 0.1.2 already, but no action wrapper wired them up -
// see PRODUCTION-ROADMAP.md §6.1 (D1).
#[tokio::test]
async fn call_resolves_trigger_message_now_that_its_wired_up() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_trigger_message(TriggerMessageRequest {
                custom_data: None,
                custom_trigger: None,
                evse: None,
                requested_message: MessageTriggerEnum::Heartbeat,
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "TriggerMessage");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: TriggerMessageResponse = TriggerMessageResponse {
        custom_data: None,
        status: TriggerMessageStatusEnum::Accepted,
        status_info: None,
    };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    call.await.unwrap().unwrap();
}

#[tokio::test]
async fn call_resolves_set_display_message_now_that_its_wired_up() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_set_display_message(SetDisplayMessageRequest {
                custom_data: None,
                message: MessageInfo {
                    custom_data: None,
                    display: None,
                    end_date_time: None,
                    id: 1,
                    message: MessageContent {
                        content: "Hello".into(),
                        custom_data: None,
                        format: MessageFormatEnum::ASCII,
                        language: None,
                    },
                    message_extra: None,
                    priority: MessagePriorityEnum::NormalCycle,
                    start_date_time: None,
                    state: None,
                    transaction_id: None,
                },
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "SetDisplayMessage");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: SetDisplayMessageResponse = SetDisplayMessageResponse {
        custom_data: None,
        status: DisplayMessageStatusEnum::Accepted,
        status_info: None,
    };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    call.await.unwrap().unwrap();
}

#[tokio::test]
async fn call_resolves_get_der_control_now_that_its_wired_up() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_get_der_control(GetDERControlRequest {
                control_id: None,
                control_type: Some(DERControlEnum::FreqDroop),
                custom_data: None,
                is_default: None,
                request_id: 1,
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "GetDERControl");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: GetDERControlResponse = GetDERControlResponse {
        custom_data: None,
        status: DERControlStatusEnum::Accepted,
        status_info: None,
    };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    call.await.unwrap().unwrap();
}

#[tokio::test]
async fn call_resolves_set_der_control_now_that_its_wired_up() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_set_der_control(SetDERControlRequest {
                control_id: "control-1".try_into().unwrap(),
                control_type: DERControlEnum::FreqDroop,
                curve: None,
                custom_data: None,
                enter_service: None,
                fixed_p_f_absorb: None,
                fixed_p_f_inject: None,
                fixed_var: None,
                freq_droop: None,
                gradient: None,
                is_default: false,
                limit_max_discharge: None,
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "SetDERControl");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: SetDERControlResponse = SetDERControlResponse {
        custom_data: None,
        status: DERControlStatusEnum::Accepted,
        status_info: None,
        superseded_ids: None,
    };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    call.await.unwrap().unwrap();
}

#[tokio::test]
async fn call_resolves_update_dynamic_schedule_now_that_its_wired_up() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let call = tokio::spawn(async move {
        client
            .send_update_dynamic_schedule(UpdateDynamicScheduleRequest {
                charging_profile_id: 1,
                custom_data: None,
                schedule_update: ChargingScheduleUpdate {
                    custom_data: None,
                    discharge_limit: None,
                    discharge_limit_l2: None,
                    discharge_limit_l3: None,
                    limit: None,
                    limit_l2: None,
                    limit_l3: None,
                    setpoint: None,
                    setpoint_l2: None,
                    setpoint_l3: None,
                    setpoint_reactive: None,
                    setpoint_reactive_l2: None,
                    setpoint_reactive_l3: None,
                },
            })
            .await
    });

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 2);
    assert_eq!(frame[2], "UpdateDynamicSchedule");
    let message_id = frame[1].as_str().unwrap().to_string();

    let response: UpdateDynamicScheduleResponse = UpdateDynamicScheduleResponse {
        custom_data: None,
        status: ChargingProfileStatusEnum::Accepted,
        status_info: None,
    };
    let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
    peer_sink.send(result_frame).await.unwrap();

    call.await.unwrap().unwrap();
}

#[tokio::test]
async fn unregistered_action_gets_not_implemented() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));
    let _keep_alive = client;

    let call_frame =
        serde_json::to_string(&json!([2, "req-2", "Reset", {"type": "Immediate"}])).unwrap();
    peer_sink.send(call_frame).await.unwrap();

    let response = recv_frame(&mut peer_source).await;
    assert_eq!(response[0], 4);
    assert_eq!(response[1], "req-2");
    assert_eq!(response[2], OCPP2_1Error::not_implemented("Reset").code());
}

#[tokio::test]
async fn send_notify_periodic_event_stream_writes_a_send_frame_and_does_not_wait_for_a_reply() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let payload = NotifyPeriodicEventStream {
        basetime: OcppTimestamp::parse_rfc3339("2024-08-27T12:30:40Z").unwrap(),
        custom_data: None,
        data: Vec::new(),
        id: 123,
        pending: 0,
    };

    // `send_notification` returns as soon as the transport accepts the frame - no waiter, no
    // timeout - so this must resolve even though nothing on `peer_sink`/`peer_source` ever
    // sends a reply.
    client
        .send_notify_periodic_event_stream(payload)
        .await
        .unwrap();

    let frame = recv_frame(&mut peer_source).await;
    assert_eq!(frame[0], 6);
    assert_eq!(frame[2], "NotifyPeriodicEventStream");
    assert_eq!(frame[3]["id"], 123);
    assert_eq!(frame[3]["pending"], 0);

    // Proves the client itself never auto-replies to its own SEND: the next thing to arrive
    // on this in-memory transport is a CALL sent afterwards, not some stray CALLRESULT/
    // CALLERROR for the SEND.
    let heartbeat = tokio::spawn(async move {
        client
            .send_heartbeat(HeartbeatRequest { custom_data: None })
            .await
    });
    let next_frame = recv_frame(&mut peer_source).await;
    assert_eq!(next_frame[0], 2);
    assert_eq!(next_frame[2], "Heartbeat");
    let message_id = next_frame[1].as_str().unwrap().to_string();
    let response: HeartbeatResponse = HeartbeatResponse {
        custom_data: None,
        current_time: OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap(),
    };
    peer_sink
        .send(serde_json::to_string(&json!([3, message_id, response])).unwrap())
        .await
        .unwrap();
    heartbeat.await.unwrap().unwrap();
}

#[tokio::test]
async fn on_notify_periodic_event_stream_fires_and_never_sends_a_reply() {
    let (client, mut peer_sink, mut peer_source) = client_pair(Duration::from_secs(5));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    client
        .on_notify_periodic_event_stream(move |payload, _client| {
            let tx = tx.clone();
            async move {
                let _ = tx.send(payload);
            }
        })
        .await;

    let send_frame = serde_json::to_string(&json!([
        6,
        "peer-1",
        "NotifyPeriodicEventStream",
        {
            "basetime": "2024-08-27T12:30:40Z",
            "data": [],
            "id": 123,
            "pending": 0,
        }
    ]))
    .unwrap();
    peer_sink.send(send_frame).await.unwrap();

    let received = rx.recv().await.unwrap();
    assert_eq!(received.id, 123);
    assert_eq!(received.pending, 0);

    // The spec forbids replying to a SEND - assert nothing comes back within a short window.
    let nothing = tokio::time::timeout(Duration::from_millis(200), peer_source.recv()).await;
    assert!(nothing.is_err(), "client must never reply to a SEND");

    let _keep_alive = client;
}
