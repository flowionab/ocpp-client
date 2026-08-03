use crate::transport::{TransportError, TransportEvent, TransportSink, TransportStream};
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
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

impl TransportSink for WebSocketSink {
    fn send<'a>(
        &'a mut self,
        frame: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move { self.0.send(Message::text(frame)).await.map_err(boxed) })
    }

    fn ping<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send(Message::Ping(Vec::new().into()))
                .await
                .map_err(boxed)
        })
    }

    fn pong<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .send(Message::Pong(Vec::new().into()))
                .await
                .map_err(boxed)
        })
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move { self.0.close().await.map_err(boxed) })
    }
}

impl TransportStream for WebSocketSource {
    fn recv<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<TransportEvent>, TransportError>> + Send + 'a>>
    {
        Box::pin(async move {
            loop {
                return match self.0.next().await {
                    None => Ok(None),
                    Some(Err(err)) => Err(boxed(err)),
                    Some(Ok(Message::Text(text))) => {
                        Ok(Some(TransportEvent::Frame(text.to_string())))
                    }
                    Some(Ok(Message::Ping(_))) => Ok(Some(TransportEvent::Ping)),
                    Some(Ok(Message::Pong(_))) => Ok(Some(TransportEvent::Pong)),
                    Some(Ok(Message::Close(_))) => Ok(None),
                    // Binary/frame frames aren't valid OCPP-J; skip and keep reading.
                    Some(Ok(_)) => continue,
                };
            }
        })
    }
}

pub(crate) fn split(stream: WsStream) -> (Box<dyn TransportSink>, Box<dyn TransportStream>) {
    let (sink, stream) = stream.split();
    (
        Box::new(WebSocketSink(sink)),
        Box::new(WebSocketSource(stream)),
    )
}
