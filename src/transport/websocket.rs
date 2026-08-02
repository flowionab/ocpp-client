use crate::transport::{TransportError, TransportEvent, TransportSink, TransportStream};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(crate) struct WebSocketSink(SplitSink<WsStream, Message>);
pub(crate) struct WebSocketSource(SplitStream<WsStream>);

fn boxed(err: tokio_tungstenite::tungstenite::Error) -> TransportError {
    Box::new(err)
}

#[async_trait::async_trait]
impl TransportSink for WebSocketSink {
    async fn send(&mut self, frame: String) -> Result<(), TransportError> {
        self.0.send(Message::text(frame)).await.map_err(boxed)
    }

    async fn ping(&mut self) -> Result<(), TransportError> {
        self.0
            .send(Message::Ping(Vec::new().into()))
            .await
            .map_err(boxed)
    }

    async fn pong(&mut self) -> Result<(), TransportError> {
        self.0
            .send(Message::Pong(Vec::new().into()))
            .await
            .map_err(boxed)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.0.close().await.map_err(boxed)
    }
}

#[async_trait::async_trait]
impl TransportStream for WebSocketSource {
    async fn recv(&mut self) -> Result<Option<TransportEvent>, TransportError> {
        loop {
            return match self.0.next().await {
                None => Ok(None),
                Some(Err(err)) => Err(boxed(err)),
                Some(Ok(Message::Text(text))) => Ok(Some(TransportEvent::Frame(text.to_string()))),
                Some(Ok(Message::Ping(_))) => Ok(Some(TransportEvent::Ping)),
                Some(Ok(Message::Pong(_))) => Ok(Some(TransportEvent::Pong)),
                Some(Ok(Message::Close(_))) => Ok(None),
                // Binary/frame frames aren't valid OCPP-J; skip and keep reading.
                Some(Ok(_)) => continue,
            };
        }
    }
}

pub(crate) fn split(stream: WsStream) -> (Box<dyn TransportSink>, Box<dyn TransportStream>) {
    let (sink, stream) = stream.split();
    (
        Box::new(WebSocketSink(sink)),
        Box::new(WebSocketSource(stream)),
    )
}
