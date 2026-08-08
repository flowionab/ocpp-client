//! `From<ValidationError>` for each version's CALLERROR type.
//!
//! `ocpp-types` 0.3.0's `validate` feature reports *what* a payload broke and classifies it as
//! `ConstraintClass::{Property, Occurrence}`, but deliberately stops there: its own docs say the
//! caller picks the wire code "from their version's `RpcErrorCode`". That last step is
//! version-specific in a way that is easy to get wrong - OCPP 1.6J spells it
//! `OccurenceConstraintViolation`, missing an `r` that 2.0.1 and 2.1 restored - so this crate,
//! which owns the three per-version error enums, is the only place it can live once.
//!
//! Not mirrored per version: the whole point is the three versions differing, so one file
//! covers all of them. Needs `--features validate`.
#![cfg(feature = "validate")]

mod common;

use common::fake_transport_pair;
use ocpp_client::ocpp_1_6::OCPP1_6Error;
use ocpp_client::ocpp_2_0_1::OCPP2_0_1Error;
use ocpp_client::ocpp_2_1::{OCPP2_1Client, OCPP2_1Error};
use ocpp_client::ocpp_types::v21::ClearVariableMonitoringRequest;
use ocpp_client::ocpp_types::v21::common::CustomData;
use ocpp_client::ocpp_types::validate::{Validate, ValidationError, ValidationErrorKind};
use ocpp_client::{
    Client, ProtocolError, TokioExecutor, TokioTimer, TransportEvent, TransportSink,
};
use serde_json::{Value, json};
use std::time::Duration;

/// A real 1.6 payload that breaks a `maxLength` the type cannot carry: `csr` is bounded at 5500
/// in the schema but is a growable `String` under `alloc`, so nothing rejects it at construction.
fn oversized_1_6_csr() -> ValidationError {
    ocpp_client::ocpp_types::v16::SignCertificateRequest {
        csr: "-".repeat(6000),
    }
    .validate()
    .expect_err("a 6000-character csr breaks maxLength: 5500")
}

/// A real 2.1 payload that breaks `minItems: 1` - the constraint no collection type expresses.
fn empty_2_1_monitor_list() -> ValidationError {
    ClearVariableMonitoringRequest::<()> {
        custom_data: None,
        id: Vec::new(),
    }
    .validate()
    .expect_err("an empty id list breaks minItems: 1")
}

#[test]
fn a_property_violation_maps_to_property_constraint_violation() {
    let error: OCPP1_6Error = oversized_1_6_csr().into();

    assert_eq!(error.code(), "PropertyConstraintViolation");
    // The description is the whole point of routing through `validate` rather than letting the
    // peer answer: it names the field the CALLERROR itself never would.
    assert!(
        error.description().contains("csr"),
        "description should name the failing field, got {:?}",
        error.description()
    );
}

/// The reason this mapping belongs in this crate rather than in each consumer. 1.6J's RPC error
/// table really does spell it `Occurence`, with one `r`; 2.0.1 and 2.1 spell it `Occurrence`. A
/// consumer hand-rolling the match would have to know that.
///
/// Built by hand rather than from a payload because no 1.6 schema states `minItems` at all -
/// there is no 1.6 message that can produce this class - but a consumer validating their own
/// custom `Action` payload can still raise it, and the mapping has to be right when they do.
#[test]
fn a_1_6_occurrence_violation_keeps_the_one_r_spelling() {
    let error: OCPP1_6Error =
        ValidationError::new(ValidationErrorKind::TooFewItems { len: 0, min: 1 }).into();

    assert_eq!(error.code(), "OccurenceConstraintViolation");
}

#[test]
fn a_2_0_1_occurrence_violation_uses_the_two_r_spelling() {
    let error: OCPP2_0_1Error =
        ValidationError::new(ValidationErrorKind::TooFewItems { len: 0, min: 1 }).into();

    assert_eq!(error.code(), "OccurrenceConstraintViolation");
}

#[test]
fn a_2_1_occurrence_violation_uses_the_two_r_spelling() {
    let error: OCPP2_1Error = empty_2_1_monitor_list().into();

    assert_eq!(error.code(), "OccurrenceConstraintViolation");
    assert!(
        error.description().contains("id"),
        "description should name the failing field, got {:?}",
        error.description()
    );
}

/// `errorDetails` has no shape in the specification, so this crate fills it with the one thing a
/// peer can act on mechanically: the JSON path, rather than a sentence it would have to parse.
#[test]
fn details_carry_the_json_path_with_array_indices() {
    // `id[0]` is -1 against a `minimum: 0`, so the path runs through an array index.
    let error: OCPP2_1Error = ClearVariableMonitoringRequest::<()> {
        custom_data: None,
        id: vec![-1],
    }
    .validate()
    .expect_err("-1 breaks minimum: 0")
    .into();

    assert_eq!(error.code(), "PropertyConstraintViolation");
    assert_eq!(error.details()["path"], json!("id[0]"));
}

/// A violation at the root of the payload has no path to print. It must still produce a usable
/// error rather than an empty string.
#[test]
fn a_rootless_violation_still_renders_a_path() {
    let error: OCPP2_1Error =
        ValidationError::new(ValidationErrorKind::TooFewItems { len: 0, min: 1 }).into();

    assert_eq!(error.details()["path"], json!("<payload>"));
}

/// `details["path"]` and `description` are produced by different code: this crate walks
/// `ValidationError::path()` to build the former, while the latter is upstream's `Display`, which
/// renders the same path and then appends the reason. Pin that the two agree, so if `ocpp-types`
/// ever changes how it writes a path, that shows up here as a failure rather than as two fields
/// quietly disagreeing on the wire.
#[test]
fn the_details_path_and_the_description_agree() {
    for error in [
        oversized_1_6_csr(),
        empty_2_1_monitor_list(),
        ClearVariableMonitoringRequest::<()> {
            custom_data: None,
            id: vec![-1],
        }
        .validate()
        .expect_err("-1 breaks minimum: 0"),
        ValidationError::new(ValidationErrorKind::TooFewItems { len: 0, min: 1 }),
    ] {
        let upstream = error.to_string();
        let mapped: OCPP2_1Error = error.into();
        let path = mapped.details()["path"].as_str().unwrap().to_string();

        assert!(
            upstream.starts_with(&format!("{path}: ")),
            "path {path:?} should prefix upstream's rendering {upstream:?}"
        );
    }
}

/// The payoff, end to end: a charge point that validates what its CSMS sent it can reject the
/// call with the correct wire code instead of hand-rolling one. Proves the conversion survives
/// the trip through `Client::on`'s handler-error path and onto the wire.
#[tokio::test]
async fn a_handler_can_reject_a_bad_payload_with_the_right_wire_code() {
    let ((client_sink, client_source), (mut peer_sink, mut peer_source)) = fake_transport_pair();
    let client: OCPP2_1Client = Client::from_transport(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(5),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
    );

    // The annotation is `CustomData`, not the bare type: `ocpp-types` defaults the parameter to
    // `NoCustomData`, while this crate's generated `on_*` methods stay concrete at the
    // specification's own shape. See the `ocpp_2_1_action!` doc comment.
    client
        .on_clear_variable_monitoring(
            |request: ClearVariableMonitoringRequest<CustomData>, _client| async move {
                // Exactly the one-liner the README recommends on the receiving side.
                request.validate()?;
                unreachable!("the payload under test is invalid, so validate() returns early")
            },
        )
        .await;

    // `id: []` decodes cleanly - `minItems` is not something serde enforces - so it reaches the
    // handler and only then fails.
    let call = json!([2, "req-1", "ClearVariableMonitoring", {"id": []}]);
    peer_sink
        .send(serde_json::to_string(&call).unwrap())
        .await
        .unwrap();

    let frame: Value = match peer_source.recv_event().await.unwrap() {
        TransportEvent::Frame(frame) => serde_json::from_str(&frame).unwrap(),
        other => panic!("expected a frame, got {other:?}"),
    };

    assert_eq!(frame[0], 4, "should be a CALLERROR");
    assert_eq!(frame[1], "req-1");
    assert_eq!(frame[2], "OccurrenceConstraintViolation");
    assert_eq!(frame[4]["path"], json!("id"));
}
