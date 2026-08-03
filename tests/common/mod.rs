use ocpp_client::{TransportError, TransportEvent, TransportSink, TransportStream};
use std::future::Future;
use std::pin::Pin;
use tokio::sync::mpsc;

/// An in-memory transport pair, so tests can drive `Client` without any real networking.
/// `fake_transport_pair()` returns two ends; frames sent into one arrive as `Frame` events
/// out the other.
pub struct FakeSink(mpsc::UnboundedSender<TransportEvent>);
pub struct FakeSource(mpsc::UnboundedReceiver<TransportEvent>);

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
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send(TransportEvent::Ping)
                .map_err(|e| Box::new(e) as TransportError)
        })
    }

    fn pong<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send(TransportEvent::Pong)
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
