//! Benchmarks the hot path this crate can regress on invisibly: `Client::call` dispatch
//! (encode request, register pending, match the reply back to it) and raw envelope
//! serde. Both go through the in-memory fake transport, so wall-clock is dominated by
//! `Client` bookkeeping and JSON (de)serialization, not real I/O.
use criterion::{Criterion, criterion_group, criterion_main};
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::{
    Client, TokioExecutor, TokioTimer, TransportError, TransportEvent, TransportSink,
    TransportStream,
};
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::{HeartbeatRequest, HeartbeatResponse};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

struct FakeSink(mpsc::UnboundedSender<TransportEvent>);
struct FakeSource(mpsc::UnboundedReceiver<TransportEvent>);

impl TransportSink for FakeSink {
    fn send<'a>(
        &'a mut self,
        frame: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send(TransportEvent::Frame(frame))
                .map_err(|e| Box::new(e) as TransportError)
        })
    }

    fn ping<'a>(
        &'a mut self,
        _payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn pong<'a>(
        &'a mut self,
        _payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}

impl TransportStream for FakeSource {
    fn recv<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<TransportEvent>, TransportError>> + Send + 'a>>
    {
        Box::pin(async move { Ok(self.0.recv().await) })
    }
}

fn fake_transport_pair() -> ((FakeSink, FakeSource), (FakeSink, FakeSource)) {
    let (a_tx, a_rx) = mpsc::unbounded_channel();
    let (b_tx, b_rx) = mpsc::unbounded_channel();
    (
        (FakeSink(a_tx), FakeSource(b_rx)),
        (FakeSink(b_tx), FakeSource(a_rx)),
    )
}

fn client_pair() -> (OCPP1_6Client, FakeSink, FakeSource) {
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

/// One `send_heartbeat` call end to end: request encode, dispatch through `Client`, a
/// peer reply crafted by hand (skipping a second `Client` so this isolates the caller
/// side), and response decode.
fn bench_heartbeat_roundtrip(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("heartbeat_call_roundtrip", |b| {
        b.to_async(&rt).iter(|| async {
            let (client, mut peer_sink, mut peer_source) = client_pair();

            let call =
                tokio::spawn(async move { client.send_heartbeat(HeartbeatRequest {}).await });

            let frame = match peer_source.recv().await.unwrap().unwrap() {
                TransportEvent::Frame(frame) => frame,
                other => panic!("expected a frame, got {other:?}"),
            };
            let frame: Value = serde_json::from_str(&frame).unwrap();
            let message_id = frame[1].as_str().unwrap().to_string();

            let response = HeartbeatResponse {
                current_time: OcppTimestamp::parse_rfc3339("2026-08-06T00:00:00Z").unwrap(),
            };
            let result_frame = serde_json::to_string(&json!([3, message_id, response])).unwrap();
            peer_sink.send(result_frame).await.unwrap();

            call.await.unwrap().unwrap()
        });
    });
}

/// Raw envelope serde with no `Client` involved, to separate JSON cost from dispatch
/// bookkeeping cost in the roundtrip number above.
fn bench_envelope_serde(c: &mut Criterion) {
    let call_json = json!([2, "abc-123", "Heartbeat", HeartbeatRequest {}]).to_string();

    c.bench_function("envelope_decode_call", |b| {
        b.iter(|| serde_json::from_str::<Value>(&call_json).unwrap());
    });

    c.bench_function("envelope_encode_result", |b| {
        b.iter(|| {
            json!([
                3,
                "abc-123",
                HeartbeatResponse {
                    current_time: OcppTimestamp::parse_rfc3339("2026-08-06T00:00:00Z").unwrap(),
                }
            ])
            .to_string()
        });
    });
}

criterion_group!(benches, bench_heartbeat_roundtrip, bench_envelope_serde);
criterion_main!(benches);
