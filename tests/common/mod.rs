// `tests/*.rs` files are each their own crate, and each one gets its own copy of this module.
// No single test crate uses every helper here, so unused-item warnings are expected rather than
// a sign of dead code.
#![allow(dead_code)]

use ocpp_client::{TransportError, TransportEvent, TransportSink, TransportStream};
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;

/// An in-memory transport pair, so tests can drive `Client` without any real networking.
/// `fake_transport_pair()` returns two ends; frames sent into one arrive as `Frame` events
/// out the other.
pub struct FakeSink(mpsc::UnboundedSender<TransportEvent>);
pub struct FakeSource(mpsc::UnboundedReceiver<TransportEvent>);

impl FakeSink {
    /// Push a raw event into this end of the pair, for events the `TransportSink` API can't
    /// produce - an unsolicited pong, say.
    pub fn send_event(&self, event: TransportEvent) {
        self.0.send(event).expect("other end still open");
    }

    /// Another handle on the same channel. The fake sink is only an `UnboundedSender`, so a test
    /// can keep one for injection while handing another to `spawn_peer`.
    pub fn duplicate(&self) -> FakeSink {
        FakeSink(self.0.clone())
    }
}

impl FakeSource {
    /// Read the next event directly, bypassing `Client`, so a test can assert on what the client
    /// actually put on the wire.
    pub async fn recv_event(&mut self) -> Option<TransportEvent> {
        self.0.recv().await
    }
}

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
        payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send(TransportEvent::Ping(payload))
                .map_err(|e| Box::new(e) as TransportError)
        })
    }

    fn pong<'a>(
        &'a mut self,
        payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send(TransportEvent::Pong(payload))
                .map_err(|e| Box::new(e) as TransportError)
        })
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

pub fn fake_transport_pair() -> ((FakeSink, FakeSource), (FakeSink, FakeSource)) {
    let (a_tx, a_rx) = mpsc::unbounded_channel();
    let (b_tx, b_rx) = mpsc::unbounded_channel();
    (
        (FakeSink(a_tx), FakeSource(b_rx)),
        (FakeSink(b_tx), FakeSource(a_rx)),
    )
}

/// How a test peer answers the pings the client sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PongBehavior {
    /// Answer every ping with a pong echoing its payload, as RFC 6455 requires.
    Echo,
    /// Never answer. Models a half-open connection: the socket still accepts writes, but
    /// nothing comes back.
    Silent,
    /// Answer, but with an empty payload instead of the ping's. Models a non-compliant peer,
    /// and pins down that correlation is by payload rather than by arrival order.
    WrongPayload,
}

/// Drives the peer end of a [`fake_transport_pair`], answering pings per `behavior`.
///
/// Returns a receiver yielding the payload of every ping the client sent, so a test can count
/// pings and assert on their correlation tokens without reaching into the client.
pub fn spawn_peer(
    sink: FakeSink,
    source: FakeSource,
    behavior: PongBehavior,
) -> mpsc::UnboundedReceiver<Vec<u8>> {
    let (observed_tx, observed_rx) = mpsc::unbounded_channel();
    let mut source = source;
    tokio::spawn(async move {
        while let Some(event) = source.0.recv().await {
            if let TransportEvent::Ping(payload) = event {
                if observed_tx.send(payload.clone()).is_err() {
                    break;
                }
                match behavior {
                    PongBehavior::Echo => {
                        let _ = sink.0.send(TransportEvent::Pong(payload));
                    }
                    PongBehavior::WrongPayload => {
                        let _ = sink.0.send(TransportEvent::Pong(Vec::new()));
                    }
                    PongBehavior::Silent => {}
                }
            }
        }
    });
    observed_rx
}
