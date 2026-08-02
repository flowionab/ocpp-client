use ocpp_client::{TransportError, TransportEvent, TransportSink, TransportStream};
use tokio::sync::mpsc;

/// An in-memory transport pair, so tests can drive `Client` without any real networking.
/// `fake_transport_pair()` returns two ends; frames sent into one arrive as `Frame` events
/// out the other.
pub struct FakeSink(mpsc::UnboundedSender<TransportEvent>);
pub struct FakeSource(mpsc::UnboundedReceiver<TransportEvent>);

#[async_trait::async_trait]
impl TransportSink for FakeSink {
    async fn send(&mut self, frame: String) -> Result<(), TransportError> {
        self.0
            .send(TransportEvent::Frame(frame))
            .map_err(|e| Box::new(e) as TransportError)
    }

    async fn ping(&mut self) -> Result<(), TransportError> {
        self.0
            .send(TransportEvent::Ping)
            .map_err(|e| Box::new(e) as TransportError)
    }

    async fn pong(&mut self) -> Result<(), TransportError> {
        self.0
            .send(TransportEvent::Pong)
            .map_err(|e| Box::new(e) as TransportError)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl TransportStream for FakeSource {
    async fn recv(&mut self) -> Result<Option<TransportEvent>, TransportError> {
        Ok(self.0.recv().await)
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
