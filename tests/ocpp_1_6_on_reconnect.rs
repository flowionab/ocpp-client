//! `Client::on_reconnect` should fire once the background read loop successfully redials after
//! a disconnect - and not before. Drives a real `Client` over the in-memory fake transport
//! (see tests/common/mod.rs), with a custom `Reconnector` that hands out a second fake pair
//! when the connection drops, so this exercises the actual reconnect-notify wiring in
//! `Client::from_transport_with_reconnect` without any real networking.
mod common;

use common::{FakeSink, FakeSource, fake_transport_pair};
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::{
    Client, ReconnectPolicy, Reconnector, TokioExecutor, TokioTimer, TransportError, TransportSink,
    TransportStream,
};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};

/// Hands out one pre-built fake transport pair the first time it's called, then always fails -
/// enough to prove a single reconnect happened without needing an unbounded supply of pairs.
struct OnceReconnector {
    pair: Mutex<Option<(FakeSink, FakeSource)>>,
}

impl Reconnector for OnceReconnector {
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
            let mut lock = self.pair.lock().await;
            match lock.take() {
                Some((sink, source)) => Ok((
                    Box::new(sink) as Box<dyn TransportSink>,
                    Box::new(source) as Box<dyn TransportStream>,
                )),
                None => Err("no more reconnect attempts configured for this test".into()),
            }
        })
    }
}

#[tokio::test]
async fn on_reconnect_fires_only_after_a_redial() {
    let ((client_sink, client_source), (peer_sink, _peer_source)) = fake_transport_pair();
    let (
        (reconnect_client_sink, reconnect_client_source),
        (_second_peer_sink, _second_peer_source),
    ) = fake_transport_pair();

    let reconnector = OnceReconnector {
        pair: Mutex::new(Some((reconnect_client_sink, reconnect_client_source))),
    };

    let client: OCPP1_6Client = Client::from_transport_with_reconnect(
        Box::new(client_sink),
        Box::new(client_source),
        Duration::from_secs(5),
        Box::new(TokioExecutor),
        Box::new(TokioTimer),
        Some(Box::new(reconnector)),
        ReconnectPolicy {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            multiplier: 2,
            jitter: false,
        },
    );

    let (fired_tx, mut fired_rx) = mpsc::unbounded_channel();
    client
        .on_reconnect(move |_client| {
            let fired_tx = fired_tx.clone();
            async move {
                let _ = fired_tx.send(());
            }
        })
        .await;

    assert!(
        fired_rx.try_recv().is_err(),
        "must not fire before any disconnect/reconnect happens"
    );

    // Drop the peer's sender to close the client's read stream, forcing a reconnect.
    drop(peer_sink);

    tokio::time::timeout(Duration::from_secs(1), fired_rx.recv())
        .await
        .expect("on_reconnect callback should fire after the background loop redials")
        .expect("channel should not be closed");
}
