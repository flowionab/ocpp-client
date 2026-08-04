//! OCPP-over-WebSocket transport over an `embassy_net::Stack`.
//!
//! Uses `embedded-websocket`'s sans-io `WebSocket` core (not its `framer_async` module) driven
//! directly over `embassy_net::tcp::{TcpReader, TcpWriter}` - see this crate's README for why,
//! and for the buffering/fragmentation/close-handshake simplifications made here that haven't
//! been validated against a real CSMS yet.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use embassy_net::tcp::{TcpReader, TcpSocket, TcpWriter};
use embassy_net::{IpEndpoint, Stack};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use embedded_websocket::{
    WebSocketClient, WebSocketCloseStatusCode, WebSocketOptions, WebSocketReceiveMessageType,
    WebSocketSendMessageType,
};
use ocpp_client::{Reconnector, TransportError, TransportEvent, TransportSink, TransportStream};
use rand_core::RngCore;

/// Builds a fresh RNG instance for each connect/reconnect attempt (RFC6455 requires client
/// frames to be masked with unpredictable data; reseeding per attempt is simplest and avoids
/// needing the RNG itself to be `Sync`). Wrap whatever RNG your board provides - a HAL hardware
/// RNG peripheral, a PRNG seeded from one at boot, etc.
pub type RngFactory = Arc<dyn Fn() -> Box<dyn RngCore + Send> + Send + Sync>;

/// Everything needed to (re)dial one OCPP WebSocket connection. Cheap to clone (used by
/// [`EmbassyNetReconnector`] to redial with the same settings after a disconnect).
#[derive(Clone)]
pub struct ConnectConfig {
    pub stack: Stack<'static>,
    pub remote: IpEndpoint,
    /// Sent as the WebSocket handshake's `Host:` header.
    pub host: String,
    /// Sent as the WebSocket handshake's request path (e.g. `/ocpp/CP001`).
    pub path: String,
    /// Sent as the requested `Sec-WebSocket-Protocol` (e.g. `"ocpp1.6"`, `"ocpp2.0.1"`,
    /// `"ocpp2.1"` - matching `connect_1_6`/`connect_2_0_1`/`connect_2_1`'s protocol strings in
    /// `ocpp-client`'s own WebSocket transport).
    pub sub_protocol: &'static str,
    pub rng_factory: RngFactory,
    /// Byte size of the raw TCP receive buffer handed to `embassy_net::tcp::TcpSocket`.
    pub socket_rx_buffer_size: usize,
    /// Byte size of the raw TCP transmit buffer handed to `embassy_net::tcp::TcpSocket`.
    pub socket_tx_buffer_size: usize,
    /// Scratch buffer for building/reading the WebSocket opening handshake's HTTP request and
    /// response. Must be large enough to hold the full HTTP response the CSMS sends back.
    pub handshake_buffer_size: usize,
    /// Scratch buffer for one WebSocket frame's decoded payload / encoded payload. Must be
    /// large enough to hold the largest OCPP-J message you expect to send or receive.
    pub frame_buffer_size: usize,
}

impl ConnectConfig {
    /// Buffer sizes default to values comfortable for typical OCPP-J message sizes; override
    /// the `*_buffer_size` fields directly on the returned value if you need more headroom.
    pub fn new(
        stack: Stack<'static>,
        remote: IpEndpoint,
        host: impl Into<String>,
        path: impl Into<String>,
        sub_protocol: &'static str,
        rng_factory: RngFactory,
    ) -> Self {
        Self {
            stack,
            remote,
            host: host.into(),
            path: path.into(),
            sub_protocol,
            rng_factory,
            socket_rx_buffer_size: 2048,
            socket_tx_buffer_size: 2048,
            handshake_buffer_size: 2048,
            frame_buffer_size: 4096,
        }
    }
}

type SharedWebSocket =
    Arc<EmbassyMutex<CriticalSectionRawMutex, WebSocketClient<Box<dyn RngCore + Send>>>>;

/// Dials `config.remote`, performs the WebSocket opening handshake, and returns boxed
/// `TransportSink`/`TransportStream` halves ready for `ocpp_client::Client::from_transport` (or
/// `_with_reconnect`, pairing with [`EmbassyNetReconnector`]).
///
/// Leaks the TCP socket and its buffers (`Box::leak`) to obtain the `'static` lifetime
/// `TransportSink`/`TransportStream` require - standard practice for a connection meant to live
/// for the rest of the firmware's run, not a per-call cost that accumulates (one leak per
/// successful connect/reconnect, matching one `Client` living for the process lifetime).
pub async fn connect(
    config: &ConnectConfig,
) -> Result<(Box<dyn TransportSink>, Box<dyn TransportStream>), TransportError> {
    let rx_buf: &'static mut [u8] =
        Box::leak(vec![0u8; config.socket_rx_buffer_size].into_boxed_slice());
    let tx_buf: &'static mut [u8] =
        Box::leak(vec![0u8; config.socket_tx_buffer_size].into_boxed_slice());
    let socket: &'static mut TcpSocket<'static> =
        Box::leak(Box::new(TcpSocket::new(config.stack, rx_buf, tx_buf)));

    socket.connect(config.remote).await.map_err(boxed_error)?;

    let (mut reader, mut writer) = socket.split();

    let rng = (config.rng_factory)();
    let mut ws: WebSocketClient<Box<dyn RngCore + Send>> =
        WebSocketClient::<Box<dyn RngCore + Send>>::new_client(rng);

    let mut handshake_buf = vec![0u8; config.handshake_buffer_size];
    let sub_protocols = [config.sub_protocol];
    let options = WebSocketOptions {
        path: &config.path,
        host: &config.host,
        origin: "",
        sub_protocols: Some(&sub_protocols),
        additional_headers: None,
    };
    let (request_len, ws_key) = ws
        .client_connect(&options, &mut handshake_buf)
        .map_err(boxed_error)?;
    writer
        .write(&handshake_buf[..request_len])
        .await
        .map_err(boxed_error)?;
    writer.flush().await.map_err(boxed_error)?;

    // The opening handshake response may not arrive in one read, and once it does the buffer
    // may also contain the start of the first WebSocket frame - `pending` below carries that
    // leftover into the stream half so it isn't lost.
    let mut response_buf = vec![0u8; config.handshake_buffer_size];
    let mut response_len = 0usize;
    let pending = loop {
        let n = reader
            .read(&mut response_buf[response_len..])
            .await
            .map_err(boxed_error)?;
        if n == 0 {
            return Err(boxed_error("connection closed during opening handshake"));
        }
        response_len += n;
        match ws.client_accept(&ws_key, &response_buf[..response_len]) {
            Ok((consumed, _sub_protocol)) => break response_buf[consumed..response_len].to_vec(),
            Err(embedded_websocket::Error::HttpHeaderIncomplete) => {
                if response_len == response_buf.len() {
                    return Err(boxed_error(
                        "opening handshake response exceeds handshake_buffer_size",
                    ));
                }
            }
            Err(e) => return Err(boxed_error(e)),
        }
    };

    let ws: SharedWebSocket = Arc::new(EmbassyMutex::new(ws));

    let sink = EmbassyWsSink {
        writer,
        ws: ws.clone(),
        tx_scratch: vec![0u8; config.frame_buffer_size].into_boxed_slice(),
    };
    let stream = EmbassyWsStream {
        reader,
        ws,
        socket_buf: vec![0u8; config.socket_rx_buffer_size].into_boxed_slice(),
        frame_buf: vec![0u8; config.frame_buffer_size].into_boxed_slice(),
        pending,
        accumulated: Vec::new(),
    };

    Ok((Box::new(sink), Box::new(stream)))
}

/// `ocpp_client::Reconnector` that redials [`connect`] with the same [`ConnectConfig`] - wire
/// this into `ocpp_client::Client::from_transport_with_reconnect` to get automatic reconnect,
/// the same way `ocpp-client`'s own WebSocket transport does for `connect_1_6`/etc.
///
/// `ocpp_client::Reconnector: Send + Sync + 'static` requires every implementor to actually be
/// `Sync`, but `ConnectConfig` carries an `embassy_net::Stack`, which - like
/// `embassy_executor::Spawner` (see `runtime::AssertSendSync`'s doc comment for the full
/// reasoning) - is deliberately `!Sync` because embassy's single-core cooperative model has no
/// real concurrent access to guard against. Same unsafe assertion, same justification, applied
/// here instead of in `runtime.rs` because `ConnectConfig` itself is a plain public struct that
/// shouldn't have to carry this wrapper in its own field type.
pub struct EmbassyNetReconnector {
    config: AssertSendSync<ConnectConfig>,
}

impl EmbassyNetReconnector {
    pub fn new(config: ConnectConfig) -> Self {
        Self {
            config: AssertSendSync(config),
        }
    }
}

impl Reconnector for EmbassyNetReconnector {
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
        Box::pin(AssertSendFuture(
            async move { connect(&self.config.0).await },
        ))
    }
}

/// See [`EmbassyNetReconnector`]'s doc comment - same "single-core, cooperative, nothing is
/// ever truly concurrent" justification as `runtime::AssertSendSync`, duplicated locally rather
/// than shared across modules since it's `pub(crate)`-scale plumbing, not public API.
#[derive(Clone, Copy)]
struct AssertSendSync<T>(T);
unsafe impl<T> Send for AssertSendSync<T> {}
unsafe impl<T> Sync for AssertSendSync<T> {}

/// Asserts `Send` for a future that isn't - same justification as `AssertSendSync` above, just
/// at the future level instead of the value level. Needed because e.g.
/// `TcpWriter::write()`'s returned `impl Future` captures a `&RefCell<..>` internally (via
/// `embassy_net::Stack`) and so isn't `Send` on its own, even once the struct holding the
/// `TcpWriter` has been asserted `Send` - that assertion doesn't propagate into anonymous
/// futures returned by methods called on it. Every `TransportSink`/`TransportStream` method
/// below wraps its `async move { .. }` block in this so the outer `Pin<Box<dyn Future<..> +
/// Send>>` the traits require type-checks.
struct AssertSendFuture<F>(F);
unsafe impl<F> Send for AssertSendFuture<F> {}
impl<F: Future> Future for AssertSendFuture<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { self.map_unchecked_mut(|s| &mut s.0) }.poll(cx)
    }
}

struct EmbassyWsSink {
    writer: TcpWriter<'static>,
    ws: SharedWebSocket,
    tx_scratch: Box<[u8]>,
}

// SAFETY: `TcpWriter` holds a `&'static RefCell<..>` (via embassy-net's `Stack`), which is
// `!Send` because `RefCell` is `!Sync` - but see `EmbassyNetReconnector`'s doc comment: under
// embassy's single-core cooperative model there's no real concurrent access for that to guard
// against. `TransportSink` only requires `Send` (not `Sync`), so this is the minimal assertion
// needed - no `Sync` impl here.
unsafe impl Send for EmbassyWsSink {}

impl TransportSink for EmbassyWsSink {
    fn send<'a>(
        &'a mut self,
        frame: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(AssertSendFuture(async move {
            let len = {
                let mut ws = self.ws.lock().await;
                ws.write(
                    WebSocketSendMessageType::Text,
                    true,
                    frame.as_bytes(),
                    &mut self.tx_scratch,
                )
                .map_err(boxed_error)?
            };
            self.writer
                .write(&self.tx_scratch[..len])
                .await
                .map_err(boxed_error)?;
            self.writer.flush().await.map_err(boxed_error)
        }))
    }

    fn ping<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(AssertSendFuture(async move {
            let len = {
                let mut ws = self.ws.lock().await;
                ws.write(
                    WebSocketSendMessageType::Ping,
                    true,
                    &[],
                    &mut self.tx_scratch,
                )
                .map_err(boxed_error)?
            };
            self.writer
                .write(&self.tx_scratch[..len])
                .await
                .map_err(boxed_error)?;
            self.writer.flush().await.map_err(boxed_error)
        }))
    }

    fn pong<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        // Empty-payload pong, matching ocpp-client's own tokio-tungstenite-based transport
        // (src/transport/websocket.rs) - neither echoes the triggering ping's payload today.
        Box::pin(AssertSendFuture(async move {
            let len = {
                let mut ws = self.ws.lock().await;
                ws.write(
                    WebSocketSendMessageType::Pong,
                    true,
                    &[],
                    &mut self.tx_scratch,
                )
                .map_err(boxed_error)?
            };
            self.writer
                .write(&self.tx_scratch[..len])
                .await
                .map_err(boxed_error)?;
            self.writer.flush().await.map_err(boxed_error)
        }))
    }

    fn close<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(AssertSendFuture(async move {
            let len = {
                let mut ws = self.ws.lock().await;
                ws.close(
                    WebSocketCloseStatusCode::NormalClosure,
                    None,
                    &mut self.tx_scratch,
                )
                .map_err(boxed_error)?
            };
            self.writer
                .write(&self.tx_scratch[..len])
                .await
                .map_err(boxed_error)?;
            self.writer.flush().await.map_err(boxed_error)
        }))
    }
}

// SAFETY: same reasoning as `EmbassyWsSink`'s `unsafe impl Send` above - `TcpReader` carries
// the same `!Send` `&'static RefCell<..>`, sound under embassy's single-core cooperative model.
unsafe impl Send for EmbassyWsStream {}

struct EmbassyWsStream {
    reader: TcpReader<'static>,
    ws: SharedWebSocket,
    /// Raw bytes read straight off the socket, not yet handed to the websocket decoder.
    socket_buf: Box<[u8]>,
    /// Scratch buffer the websocket decoder writes one frame chunk's payload into.
    frame_buf: Box<[u8]>,
    /// Socket bytes read but not yet fully consumed by the websocket decoder (e.g. the decoder
    /// only had room in `frame_buf` for part of a frame, or a frame boundary fell mid-buffer).
    pending: Vec<u8>,
    /// Payload assembled so far for a Text message split across multiple websocket frames
    /// (fragmentation) or multiple decoder calls (frame_buf too small for one frame).
    accumulated: Vec<u8>,
}

impl TransportStream for EmbassyWsStream {
    fn recv<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<TransportEvent>, TransportError>> + Send + 'a>>
    {
        Box::pin(AssertSendFuture(async move {
            loop {
                if self.pending.is_empty() {
                    let n = self
                        .reader
                        .read(&mut self.socket_buf)
                        .await
                        .map_err(boxed_error)?;
                    if n == 0 {
                        return Ok(None);
                    }
                    self.pending.extend_from_slice(&self.socket_buf[..n]);
                }

                let ws_result = {
                    let mut ws = self.ws.lock().await;
                    ws.read(&self.pending, &mut self.frame_buf)
                        .map_err(boxed_error)?
                };
                self.pending.drain(..ws_result.len_from);

                match ws_result.message_type {
                    WebSocketReceiveMessageType::Text => {
                        self.accumulated
                            .extend_from_slice(&self.frame_buf[..ws_result.len_to]);
                        if ws_result.end_of_message {
                            let payload = core::mem::take(&mut self.accumulated);
                            let text = String::from_utf8(payload).map_err(boxed_error)?;
                            return Ok(Some(TransportEvent::Frame(text)));
                        }
                    }
                    WebSocketReceiveMessageType::Binary => {
                        // OCPP-J is a text-only protocol (JSON over WebSocket text frames);
                        // drop anything binary instead of misinterpreting it.
                        self.accumulated.clear();
                        tracing::warn!(
                            "ocpp-transport-embassy-net: dropping unexpected binary frame"
                        );
                    }
                    WebSocketReceiveMessageType::Ping => return Ok(Some(TransportEvent::Ping)),
                    WebSocketReceiveMessageType::Pong => return Ok(Some(TransportEvent::Pong)),
                    WebSocketReceiveMessageType::CloseMustReply
                    | WebSocketReceiveMessageType::CloseCompleted => {
                        // Simplified close handling: no close-reply frame is sent back (see
                        // this crate's README) - the stream just ends here, same as a plain
                        // disconnect. `Client`'s reconnect logic (if enabled) takes it from
                        // there.
                        return Ok(None);
                    }
                }
            }
        }))
    }
}

/// Turns any `Debug + Send + Sync + 'static` error into a boxed `core::error::Error`, so this
/// module doesn't need a hand-written `Display`/`Error` impl per foreign error type
/// (`embassy_net::tcp::{Error, ConnectError}`, `embedded_websocket::Error`, ...). `Display`
/// falls back to `{:?}` - less polished than a tailored message, but correct, and this is a
/// scaffold; revisit if callers need nicer error text.
#[derive(Debug)]
struct DebugError<E>(E);

impl<E: fmt::Debug> fmt::Display for DebugError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<E: fmt::Debug> core::error::Error for DebugError<E> {}

fn boxed_error<E: fmt::Debug + Send + Sync + 'static>(e: E) -> TransportError {
    Box::new(DebugError(e))
}
