//! OCPP-over-WebSocket transport over an `embassy_net::Stack`, with optional TLS (`wss://`).
//!
//! Uses `embedded-websocket`'s sans-io `WebSocket` core (not its `framer_async` module) driven
//! directly over `embassy_net::tcp::{TcpReader, TcpWriter}` - see this crate's README for why,
//! and for the buffering/fragmentation/close-handshake simplifications made here that haven't
//! been validated against a real CSMS yet.
//!
//! When [`ConnectConfig::tls`] is set the TCP socket is wrapped in an `embedded-tls`
//! [`TlsConnection`] *before* the WebSocket handshake, giving `wss://` with the same
//! sans-io WebSocket layer. See [`TlsConfig`]/[`TlsVerify`] for the certificate-verification
//! options and their limitations.

use alloc::boxed::Box;
use alloc::rc::Rc;
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
use embedded_io_async::{Read as AsyncRead, Write as AsyncWrite};
use embedded_tls::{
    Aes128GcmSha256, Certificate, CryptoProvider, CryptoRng, CryptoRngCore, NoClock, TlsConnection,
    TlsContext, TlsError, TlsReader, TlsVerifier, TlsWriter, UnsecureProvider, pki::CertVerifier,
};
use embedded_websocket::{
    WebSocketClient, WebSocketCloseStatusCode, WebSocketOptions, WebSocketReceiveMessageType,
    WebSocketSendMessageType,
};
use ocpp_client::{Reconnector, TransportError, TransportEvent, TransportSink, TransportStream};
use rand_core::RngCore;

// ---------------------------------------------------------------------------
// Recycled connect-attempt allocations
// ---------------------------------------------------------------------------
//
// Every (re)connect needs ~4 KiB of TCP buffers and, on `wss://`, ~19 KiB
// more of TLS record buffers. `TcpSocket`/`TlsConnection` demand `&'static`
// slices, so the straightforward implementation `Box::leak`s them per
// attempt — which not only grows the heap forever but, worse, leaks the
// socket itself: `TcpSocket`'s `Drop` is what removes it from the stack's
// `SocketSet`, so a leaked socket holds its slot forever and a few failed
// attempts make `TcpSocket::new` panic on a full set.
//
// Instead, connect-attempt buffers are leased from [`BUF_POOL`]: a global
// freelist of `Box<[u8]>`es. While leased, a buffer is handed to the
// socket/TLS layer as a `&'static mut` slice; when the owning guard drops —
// i.e. when the last reader/writer half referencing that connection is
// dropped — the buffer returns to the pool for the next attempt. The heap
// cost of reconnecting is therefore bounded by the number of *concurrently
// alive* connections, not by the number of attempts.
//
/// Freed connect-attempt buffers awaiting reuse.
static BUF_POOL: EmbassyMutex<CriticalSectionRawMutex, Vec<Box<[u8]>>> =
    EmbassyMutex::new(Vec::new());

/// Pool capacity, in buffers. Sized for ~2 connections' worth (a new
/// connection can briefly overlap an old one during a courtesy close).
const BUF_POOL_CAP: usize = 10;

/// A heap slice leased out of [`BUF_POOL`] (or freshly allocated when the
/// pool holds none of the right size). Hands its memory to the socket/TLS
/// layer as `&'static mut`, and reclaims it into the pool on drop.
struct PooledBuf {
    ptr: *mut u8,
    len: usize,
}

impl PooledBuf {
    /// Takes a buffer of exactly `len` bytes: recycled from [`BUF_POOL`]
    /// when possible, freshly allocated otherwise.
    async fn take(len: usize) -> Self {
        let mut pool = BUF_POOL.lock().await;
        match pool.iter().position(|b| b.len() == len) {
            Some(i) => {
                let b = pool.swap_remove(i);
                Self {
                    ptr: Box::into_raw(b).cast::<u8>(),
                    len,
                }
            }
            None => Self {
                ptr: Box::into_raw(vec![0u8; len].into_boxed_slice()).cast::<u8>(),
                len,
            },
        }
    }

    /// Views the leased memory as a `&'static mut` slice for the
    /// socket/TLS layer.
    ///
    /// SAFETY: the memory is exclusively owned by this `PooledBuf` — it was
    /// `Box::into_raw`'d by us and only returns to the pool from `Drop`,
    /// which runs after every user of the slice is gone (the guards that
    /// hold the `PooledBuf` outlive the halves that borrow the slice).
    /// No two live `PooledBuf`s ever cover the same memory.
    fn as_slice(&self) -> &'static mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for PooledBuf {
    fn drop(&mut self) {
        // SAFETY: `ptr`/`len` describe the exact box we `into_raw`'d in
        // `take`, and nothing references the memory anymore (see
        // `as_slice`).
        let buf = unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.ptr, self.len)) };
        let Ok(mut pool) = BUF_POOL.try_lock() else {
            return;
        };
        if pool.len() < BUF_POOL_CAP {
            pool.push(buf);
        }
    }
}

/// Owns a `TcpSocket` (and the buffers it was built from) for as long as
/// any half of the connection is alive. The socket's `Drop` removes it
/// from the stack's `SocketSet` — leaking it is what used to fill the set
/// — and dropping it must only happen once every `TcpReader`/`TcpWriter`
/// (and any `TlsConnection` layered on top) is gone, which the `Rc`s in
/// [`TcpDuplex`] guarantee.
struct SocketGuard {
    socket: *mut TcpSocket<'static>,
    _bufs: Vec<PooledBuf>,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        // SAFETY: this runs when the last `Rc<SocketGuard>` drops, and
        // every clone lives in a `TcpDuplex` reachable from a live half —
        // so no `TcpReader`/`TcpWriter`/`TlsConnection` can still touch
        // the socket. The `&'static mut` created in `connect` is dead
        // along with them.
        drop(unsafe { Box::from_raw(self.socket) });
    }
}

/// Owns a `TlsConnection` (and its record buffers) on the same refcount
/// discipline as [`SocketGuard`]: `TlsConnection::split` needs `&'static`,
/// so the connection is boxed and its raw pointer kept here; the `Rc`
/// clones in the sink/stream keep it alive until both halves are dropped.
struct TlsGuard {
    conn: *mut TlsConnection<'static, TcpDuplex, Aes128GcmSha256>,
    _rec_bufs: Vec<PooledBuf>,
}

impl Drop for TlsGuard {
    fn drop(&mut self) {
        // SAFETY: same argument as `SocketGuard` — the last clone holder
        // is gone, so neither `TlsReader` nor `TlsWriter` can touch the
        // connection anymore.
        drop(unsafe { Box::from_raw(self.conn) });
    }
}

/// Builds a fresh RNG instance for each connect/reconnect attempt (RFC6455 requires client
/// frames to be masked with unpredictable data; reseeding per attempt is simplest and avoids
/// needing the RNG itself to be `Sync`). Wrap whatever RNG your board provides - a HAL hardware
/// RNG peripheral, a PRNG seeded from one at boot, etc.
pub type RngFactory = Arc<dyn Fn() -> Box<dyn RngCore + Send> + Send + Sync>;

/// How (and whether) the server certificate is verified when [`ConnectConfig::tls`] is set.
#[derive(Clone, Copy)]
pub enum TlsVerify<'a> {
    /// **No verification at all** - the TLS layer encrypts but any certificate is accepted,
    /// leaving the connection open to MITM. Acceptable for lab/bench bring-up against a
    /// self-signed CSMS, not for production.
    None,
    /// The server's certificate chain must terminate at this DER-encoded root CA and the
    /// certificate's CommonName must equal [`ConnectConfig::host`].
    ///
    /// Known limitations, inherited from `embedded-tls`'s `pki` verifier (the same one
    /// `reqwless` uses for its `TlsVerify::Certificate`):
    /// - **Validity period is not checked** - this transport targets microcontrollers
    ///   without an RTC, so there is no trustworthy "now" to check against. Expired (or
    ///   not-yet-valid) certificates that otherwise chain correctly are accepted.
    /// - **Hostname is matched against the Subject CommonName only** - Subject
    ///   Alternative Names are not consulted, so SAN-only certificates (no CN) fail.
    /// - **The chain the server sends must walk back to the configured root** - missing
    ///   intermediates are not fetched.
    RootCaDer(&'a [u8]),
}

/// TLS settings for `wss://` endpoints. `None` on [`ConnectConfig::tls`] keeps the connection
/// plaintext (`ws://`).
#[derive(Clone)]
pub struct TlsConfig {
    /// Server-certificate verification mode - see [`TlsVerify`] for the trade-offs.
    pub verify: TlsVerify<'static>,
    /// TLS record read buffer, in bytes. Must fit one full encrypted TLS record *and* the
    /// server's handshake flight (Certificate + friends, typically 4-8 KiB); 16640 is the
    /// protocol maximum and the safe choice.
    pub record_read_buffer_size: usize,
    /// TLS record write buffer, in bytes. Writes are chunked to fit, so this can stay small;
    /// it still has to hold the ClientHello, so don't go below ~1 KiB.
    pub record_write_buffer_size: usize,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            verify: TlsVerify::None,
            record_read_buffer_size: 16640,
            record_write_buffer_size: 2048,
        }
    }
}

/// Everything needed to (re)dial one OCPP WebSocket connection. Cheap to clone (used by
/// [`EmbassyNetReconnector`] to redial with the same settings after a disconnect).
#[derive(Clone)]
pub struct ConnectConfig {
    pub stack: Stack<'static>,
    pub remote: IpEndpoint,
    /// Sent as the WebSocket handshake's `Host:` header, and - when [`Self::tls`] is set -
    /// as the TLS SNI server name and the hostname the certificate is verified against.
    pub host: String,
    /// Sent as the WebSocket handshake's request path (e.g. `/ocpp/CP001`).
    pub path: String,
    /// Sent as the requested `Sec-WebSocket-Protocol` (e.g. `"ocpp1.6"`, `"ocpp2.0.1"`,
    /// `"ocpp2.1"` - matching `connect_1_6`/`connect_2_0_1`/`connect_2_1`'s protocol strings in
    /// `ocpp-client`'s own WebSocket transport).
    pub sub_protocol: &'static str,
    pub rng_factory: RngFactory,
    /// TLS settings for `wss://`. `None` = plaintext `ws://`.
    ///
    /// Budget roughly `record_read_buffer_size + record_write_buffer_size` (~19 KiB with
    /// defaults) of extra heap per connection on top of the TCP socket buffers.
    pub tls: Option<TlsConfig>,
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
            tls: None,
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
/// When `config.tls` is set the TCP socket is upgraded to `embedded-tls` *before* the
/// WebSocket handshake (i.e. `wss://`).
///
/// The socket and its buffers are not leaked: `TcpSocket::new` needs `&'static` buffers and
/// `split` needs `&'static` on the socket, so both live behind refcounted guards
/// ([`SocketGuard`]/[`TlsGuard`], buffers via [`PooledBuf`]). A failed attempt drops the
/// guard immediately — freeing the `SocketSet` slot and recycling the buffers — and a
/// successful one frees them when the client drops the transport halves.
pub async fn connect(
    config: &ConnectConfig,
) -> Result<(Box<dyn TransportSink>, Box<dyn TransportStream>), TransportError> {
    let rx = PooledBuf::take(config.socket_rx_buffer_size).await;
    let tx = PooledBuf::take(config.socket_tx_buffer_size).await;
    let socket_ptr =
        Box::into_raw(Box::new(TcpSocket::new(config.stack, rx.as_slice(), tx.as_slice())));
    let guard = Rc::new(SocketGuard {
        socket: socket_ptr,
        _bufs: vec![rx, tx],
    });
    // SAFETY: `SocketGuard` keeps the box alive (and untouched) until the
    // last half derived from this `&'static mut` is dropped - see the
    // guard's `Drop`.
    let socket: &'static mut TcpSocket<'static> = unsafe { &mut *socket_ptr };

    socket.connect(config.remote).await.map_err(boxed_error)?;

    let (reader, writer) = socket.split();
    let duplex = TcpDuplex::new(reader, writer, guard);

    match config.tls.clone() {
        Some(tls) => {
            let (reader, writer, tls_guard) = open_tls(duplex, config, &tls).await?;
            ws_handshake_and_build(reader, writer, config, Some(tls_guard)).await
        }
        None => ws_handshake_and_build(duplex.clone(), duplex, config, None).await,
    }
}

/// Opens a TLS session over the TCP halves and returns `embedded_io_async` reader/writer
/// halves of the *TLS* connection, ready to feed the same sans-io WebSocket layer the
/// plaintext path uses.
///
/// `TlsConnection::split` requires the underlying socket to be `Clone` (both halves may
/// generate raw-TCP traffic - e.g. a read path answering a KeyUpdate). `embassy_net`'s
/// `TcpSocket` is not `Clone`, so the TCP halves live behind per-direction `Rc<Mutex>`s in
/// [`TcpDuplex`]: reads and writes go to *different* mutexes, so a parked TLS read never
/// blocks a concurrent TLS write (unlike a single mutex around the whole socket, which would
/// deadlock). Both clones share each direction's mutex so a second concurrent borrow waits
/// cooperatively rather than panicking.
async fn open_tls(
    duplex: TcpDuplex,
    config: &ConnectConfig,
    tls: &TlsConfig,
) -> Result<
    (
        TlsReader<'static, TcpDuplex, Aes128GcmSha256>,
        TlsWriter<'static, TcpDuplex, Aes128GcmSha256>,
        Rc<TlsGuard>,
    ),
    TransportError,
> {
    let rec_rx = PooledBuf::take(tls.record_read_buffer_size).await;
    let rec_tx = PooledBuf::take(tls.record_write_buffer_size).await;
    let conn_ptr = Box::into_raw(Box::new(TlsConnection::new(
        duplex,
        rec_rx.as_slice(),
        rec_tx.as_slice(),
    )));
    let guard = Rc::new(TlsGuard {
        conn: conn_ptr,
        _rec_bufs: vec![rec_rx, rec_tx],
    });
    // SAFETY: `TlsGuard` keeps the box alive (and untouched) until the
    // last half derived from this `&'static mut` is dropped - see the
    // guard's `Drop`.
    let connection: &'static mut TlsConnection<'static, TcpDuplex, Aes128GcmSha256> =
        unsafe { &mut *conn_ptr };

    let tls_config = embedded_tls::TlsConfig::new().with_server_name(&config.host);
    // TLS needs a CSPRNG for its ephemeral key-share - `RngFactory`'s contract already requires
    // a real hardware RNG (WebSocket masking has the same requirement), so the adapter only
    // adds the `CryptoRng` marker the TLS side asks for.
    let rng = CryptoRngAdapter((config.rng_factory)());

    match tls.verify {
        TlsVerify::None => {
            connection
                .open(TlsContext::new(
                    &tls_config,
                    UnsecureProvider::new::<Aes128GcmSha256>(rng),
                ))
                .await
                .map_err(boxed_error)?;
        }
        TlsVerify::RootCaDer(ca) => {
            let tls_config = tls_config.with_ca(Certificate::X509(ca));
            connection
                .open(TlsContext::new(
                    &tls_config,
                    VerifiedProvider {
                        rng,
                        verifier: CertVerifier::new(),
                    },
                ))
                .await
                .map_err(boxed_error)?;
        }
    }

    let (reader, writer) = connection.split();
    Ok((reader, writer, guard))
}

/// A `Clone`able `embedded_io_async` socket facade over `embassy_net`'s TCP halves, required
/// by `TlsConnection::split` (see [`open_tls`]). Each direction gets its own mutex because
/// `TlsReader` parks awaiting bytes while `TlsWriter` may need to flush a record.
#[derive(Clone)]
struct TcpDuplex {
    rx: Rc<EmbassyMutex<CriticalSectionRawMutex, TcpReader<'static>>>,
    tx: Rc<EmbassyMutex<CriticalSectionRawMutex, TcpWriter<'static>>>,
    /// Keeps the underlying `TcpSocket` (and its buffers) alive while any
    /// half is alive - dropping the last clone frees the `SocketSet` slot.
    _guard: Rc<SocketGuard>,
}

impl TcpDuplex {
    fn new(
        reader: TcpReader<'static>,
        writer: TcpWriter<'static>,
        guard: Rc<SocketGuard>,
    ) -> Self {
        Self {
            rx: Rc::new(EmbassyMutex::new(reader)),
            tx: Rc::new(EmbassyMutex::new(writer)),
            _guard: guard,
        }
    }
}

impl embedded_io_async::ErrorType for TcpDuplex {
    type Error = embassy_net::tcp::Error;
}

impl AsyncRead for TcpDuplex {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.rx.lock().await.read(buf).await
    }
}

impl AsyncWrite for TcpDuplex {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.tx.lock().await.write(buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.tx.lock().await.flush().await
    }
}

/// Marks the firmware-provided RNG as a `CryptoRng` for embedded-tls's `CryptoProvider`.
/// `RngFactory`'s contract is already "a real hardware RNG", so this adds no randomness
/// guarantees - it only satisfies the trait bound.
struct CryptoRngAdapter(Box<dyn RngCore + Send>);

impl RngCore for CryptoRngAdapter {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest);
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.0.try_fill_bytes(dest)
    }
}

// Justification documented on the struct: the factory contract already demands a CSPRNG.
impl CryptoRng for CryptoRngAdapter {}

/// `CryptoProvider` for certificate-verified connections (`TlsVerify::RootCaDer`): the
/// firmware RNG plus `embedded-tls`'s `pki` verifier (chain + CommonName; expiry unchecked
/// under `NoClock` - see [`TlsVerify`]'s docs).
struct VerifiedProvider {
    rng: CryptoRngAdapter,
    verifier: CertVerifier<Aes128GcmSha256, NoClock, 4096>,
}

impl CryptoProvider for VerifiedProvider {
    type CipherSuite = Aes128GcmSha256;
    type Signature = p256::ecdsa::DerSignature;

    fn rng(&mut self) -> impl CryptoRngCore {
        &mut self.rng
    }

    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.verifier)
    }
}

/// Runs the WebSocket opening handshake over `reader`/`writer` (TCP halves for `ws://`, TLS
/// halves for `wss://`) and assembles the boxed `TransportSink`/`TransportStream` pair.
async fn ws_handshake_and_build<R, W>(
    mut reader: R,
    mut writer: W,
    config: &ConnectConfig,
    tls_guard: Option<Rc<TlsGuard>>,
) -> Result<(Box<dyn TransportSink>, Box<dyn TransportStream>), TransportError>
where
    R: AsyncRead + 'static,
    W: AsyncWrite + 'static,
    R::Error: fmt::Debug + Send + Sync + 'static,
    W::Error: fmt::Debug + Send + Sync + 'static,
{
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
        .write_all(&handshake_buf[..request_len])
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
        _tls_guard: tls_guard.clone(),
    };
    let stream = EmbassyWsStream {
        reader,
        ws,
        socket_buf: vec![0u8; config.socket_rx_buffer_size].into_boxed_slice(),
        frame_buf: vec![0u8; config.frame_buffer_size].into_boxed_slice(),
        pending,
        accumulated: Vec::new(),
        _tls_guard: tls_guard,
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

struct EmbassyWsSink<W> {
    writer: W,
    ws: SharedWebSocket,
    tx_scratch: Box<[u8]>,
    /// Keeps the `TlsConnection` (and its record buffers) alive while this
    /// half is alive; `None` on the plaintext `ws://` path.
    _tls_guard: Option<Rc<TlsGuard>>,
}

// SAFETY: `W` is a `TcpWriter` or `TlsWriter`, both of which hold `!Send` interior state
// (`&'static RefCell<..>` via embassy-net's `Stack`, `Rc<..>` via `TcpDuplex`) - but see
// `EmbassyNetReconnector`'s doc comment: under embassy's single-core cooperative model there's
// no real concurrent access for that to guard against. `TransportSink` only requires `Send`
// (not `Sync`), so this is the minimal assertion needed - no `Sync` impl here.
unsafe impl<W> Send for EmbassyWsSink<W> {}

impl<W> TransportSink for EmbassyWsSink<W>
where
    W: AsyncWrite + 'static,
    W::Error: fmt::Debug + Send + Sync + 'static,
{
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
                .write_all(&self.tx_scratch[..len])
                .await
                .map_err(boxed_error)?;
            self.writer.flush().await.map_err(boxed_error)
        }))
    }

    fn ping<'a>(
        &'a mut self,
        payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        // The payload carries `Client`'s correlation token; it has to go out verbatim or the
        // pong that echoes it back can't be matched to the ping that asked for it.
        Box::pin(AssertSendFuture(async move {
            let len = {
                let mut ws = self.ws.lock().await;
                ws.write(
                    WebSocketSendMessageType::Ping,
                    true,
                    &payload,
                    &mut self.tx_scratch,
                )
                .map_err(boxed_error)?
            };
            self.writer
                .write_all(&self.tx_scratch[..len])
                .await
                .map_err(boxed_error)?;
            self.writer.flush().await.map_err(boxed_error)
        }))
    }

    fn pong<'a>(
        &'a mut self,
        payload: Vec<u8>,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        // Echoes the triggering ping's payload, as RFC 6455 §5.5.3 requires - `Client`'s read
        // loop hands it straight through from the `TransportEvent::Ping` it is replying to.
        Box::pin(AssertSendFuture(async move {
            let len = {
                let mut ws = self.ws.lock().await;
                ws.write(
                    WebSocketSendMessageType::Pong,
                    true,
                    &payload,
                    &mut self.tx_scratch,
                )
                .map_err(boxed_error)?
            };
            self.writer
                .write_all(&self.tx_scratch[..len])
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
                .write_all(&self.tx_scratch[..len])
                .await
                .map_err(boxed_error)?;
            self.writer.flush().await.map_err(boxed_error)
        }))
    }
}

// SAFETY: same reasoning as `EmbassyWsSink`'s `unsafe impl Send` above - `R` is a `TcpReader`
// or `TlsReader`, carrying the same kind of `!Send` interior state, sound under embassy's
// single-core cooperative model.
unsafe impl<R> Send for EmbassyWsStream<R> {}

struct EmbassyWsStream<R> {
    reader: R,
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
    /// See `EmbassyWsSink::_tls_guard`.
    _tls_guard: Option<Rc<TlsGuard>>,
}

impl<R> TransportStream for EmbassyWsStream<R>
where
    R: AsyncRead + 'static,
    R::Error: fmt::Debug + Send + Sync + 'static,
{
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
                    // Control-frame payloads are capped at 125 bytes by RFC 6455, so these always
                    // arrive whole in `frame_buf` - no accumulation needed, unlike Text.
                    WebSocketReceiveMessageType::Ping => {
                        return Ok(Some(TransportEvent::Ping(
                            self.frame_buf[..ws_result.len_to].to_vec(),
                        )));
                    }
                    WebSocketReceiveMessageType::Pong => {
                        return Ok(Some(TransportEvent::Pong(
                            self.frame_buf[..ws_result.len_to].to_vec(),
                        )));
                    }
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// `ws_handshake_and_build` is generic over `embedded_io_async::{Read, Write}`
// exactly so this module can drive the WebSocket layer over in-memory halves
// — no `embassy_net` socket required. `NullDriver` exists only so
// `embassy_net::new` can hand out a `Stack` for `ConnectConfig`; the `Runner`
// it pairs with is never polled, so no packet ever moves.

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use alloc::collections::VecDeque;
    use alloc::string::ToString;
    use core::cell::RefCell;
    use embedded_io_async::ErrorType;
    use embedded_websocket::{WebSocketKey, WebSocketServer, WebSocketSubProtocol};

    /// Deterministic xorshift64 — keeps the client frame masks (and therefore
    /// the captured wire bytes) reproducible.
    struct TestRng(u64);

    impl RngCore for TestRng {
        fn next_u32(&mut self) -> u32 {
            self.next_u64() as u32
        }
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            self.try_fill_bytes(dest).expect("infallible");
        }
        fn try_fill_bytes(&mut self, mut dest: &mut [u8]) -> Result<(), rand_core::Error> {
            while !dest.is_empty() {
                let n = self.next_u64().to_le_bytes();
                let k = dest.len().min(8);
                dest[..k].copy_from_slice(&n[..k]);
                dest = &mut dest[k..];
            }
            Ok(())
        }
    }

    /// Reports the link up but never produces a packet. Only exists so
    /// `embassy_net::new` can hand out a `Stack` for `ConnectConfig` — the
    /// `Runner` it pairs with is never polled.
    struct NullDriver;

    struct NullToken;
    impl embassy_net::driver::RxToken for NullToken {
        fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, _f: F) -> R {
            unreachable!("NullDriver never yields tokens")
        }
    }
    impl embassy_net::driver::TxToken for NullToken {
        fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
            f(&mut vec![0u8; len])
        }
    }
    impl embassy_net::driver::Driver for NullDriver {
        type RxToken<'a> = NullToken;
        type TxToken<'a> = NullToken;
        fn receive(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
            None
        }
        fn transmit(&mut self, _cx: &mut Context<'_>) -> Option<Self::TxToken<'_>> {
            None
        }
        fn link_state(&mut self, _cx: &mut Context<'_>) -> embassy_net::driver::LinkState {
            embassy_net::driver::LinkState::Up
        }
        fn capabilities(&self) -> embassy_net::driver::Capabilities {
            let mut caps = embassy_net::driver::Capabilities::default();
            caps.max_transmission_unit = 1536;
            caps.max_burst_size = Some(1);
            caps
        }
        fn hardware_address(&self) -> embassy_net::driver::HardwareAddress {
            embassy_net::driver::HardwareAddress::Ethernet([0x02, 0, 0, 0, 0, 1])
        }
    }

    /// Bytes the client wrote, shared between the halves so the reader can
    /// answer the request it actually observed.
    type Capture = Rc<RefCell<Vec<u8>>>;

    /// The writer half: everything the handshake/`send` emits lands in the
    /// capture.
    struct MockWriter(Capture);

    impl ErrorType for MockWriter {
        type Error = embedded_io_async::ErrorKind;
    }
    impl AsyncWrite for MockWriter {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }
        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    /// The reader half: replays scripted chunks. When `respond_101` is set the
    /// first read parses the request the client actually wrote and
    /// synthesizes the server's `101 Switching Protocols` from its
    /// `Sec-WebSocket-Key` — the accept check only passes because the answer
    /// is computed from the captured key, as a real server does.
    struct MockReader {
        capture: Capture,
        respond_101: bool,
        /// First read returns only this many bytes of the 101 — exercises the
        /// `HttpHeaderIncomplete` accumulation loop.
        response_split: Option<usize>,
        /// Appended right after the 101 — simulates the first WebSocket frame
        /// arriving in the same socket read as the handshake tail.
        piggyback: Vec<u8>,
        /// Reads after the 101; each chunk is returned whole (keep them
        /// smaller than the buffers under test). Empty queue = EOF.
        chunks: VecDeque<Vec<u8>>,
    }

    impl ErrorType for MockReader {
        type Error = embedded_io_async::ErrorKind;
    }
    impl AsyncRead for MockReader {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            if self.respond_101 {
                self.respond_101 = false;
                let request = self.capture.borrow().clone();
                let mut response = accept_101(&request);
                response.extend_from_slice(&self.piggyback);
                if let Some(k) = self.response_split {
                    let rest = response.split_off(k.min(response.len()));
                    self.chunks.push_front(rest);
                }
                self.chunks.push_front(response);
            }
            match self.chunks.pop_front() {
                Some(chunk) => {
                    let n = buf.len().min(chunk.len());
                    buf[..n].copy_from_slice(&chunk[..n]);
                    Ok(n)
                }
                None => Ok(0),
            }
        }
    }

    /// Parses the captured request's `Sec-WebSocket-Key` and builds the
    /// spec-valid 101 answer for it.
    fn accept_101(request: &[u8]) -> Vec<u8> {
        let request = core::str::from_utf8(request).expect("request is not utf-8");
        const NEEDLE: &str = "Sec-WebSocket-Key: ";
        let key_start = request
            .find(NEEDLE)
            .map(|i| i + NEEDLE.len())
            .expect("request has no Sec-WebSocket-Key");
        let key = WebSocketKey::from(&request[key_start..key_start + 24]);
        let mut server = WebSocketServer::new_server();
        let sub_protocol = WebSocketSubProtocol::from("ocpp1.6");
        let mut buf = [0u8; 1024];
        let len = server
            .server_accept(&key, Some(&sub_protocol), &mut buf)
            .expect("failed to build 101 response");
        buf[..len].to_vec()
    }

    fn test_stack() -> Stack<'static> {
        let resources = Box::leak(Box::new(embassy_net::StackResources::<2>::new()));
        // `Config::default()` configures no addresses at all — fine: the
        // Runner is never polled, so nothing ever asks smoltcp for one.
        let (stack, _runner) = embassy_net::new(
            NullDriver,
            embassy_net::Config::default(),
            resources,
            0x5EED,
        );
        stack
    }

    fn test_config(tls: Option<TlsConfig>) -> ConnectConfig {
        ConnectConfig {
            stack: test_stack(),
            remote: IpEndpoint::new(
                embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::new(10, 0, 0, 1)),
                8180,
            ),
            host: "csms.example.com".into(),
            path: "/ocpp/CP001".into(),
            sub_protocol: "ocpp1.6",
            rng_factory: Arc::new(|| Box::new(TestRng(0x5EED_5EED)) as Box<dyn RngCore + Send>),
            tls,
            socket_rx_buffer_size: 512,
            socket_tx_buffer_size: 512,
            handshake_buffer_size: 1024,
            frame_buffer_size: 256,
        }
    }

    fn mock_reader(chunks: &[&[u8]]) -> MockReader {
        MockReader {
            // Replaced by `dial` — the reader must share the writer's capture.
            capture: Rc::new(RefCell::new(Vec::new())),
            respond_101: true,
            response_split: None,
            piggyback: Vec::new(),
            chunks: chunks.iter().map(|c| c.to_vec()).collect(),
        }
    }

    type TransportPair = (Box<dyn TransportSink>, Box<dyn TransportStream>);

    /// Drives `ws_handshake_and_build` over the in-memory halves.
    fn dial(
        config: &ConnectConfig,
        mut reader: MockReader,
    ) -> (Result<TransportPair, TransportError>, Capture) {
        let capture: Capture = Rc::new(RefCell::new(Vec::new()));
        reader.capture = capture.clone();
        let writer = MockWriter(capture.clone());
        let result =
            futures::executor::block_on(ws_handshake_and_build(reader, writer, config, None));
        (result, capture)
    }

    /// `dial`, expecting success.
    fn handshake(
        config: &ConnectConfig,
        reader: MockReader,
    ) -> (Box<dyn TransportSink>, Box<dyn TransportStream>, Capture) {
        let (result, capture) = dial(config, reader);
        let (sink, stream) = result.expect("opening handshake failed");
        (sink, stream, capture)
    }

    /// Decodes one client→server frame (masked, per RFC 6455) and returns
    /// (opcode, payload). Handles short payloads only (`len < 126`) — enough
    /// for these tests.
    fn decode_client_frame(frame: &[u8]) -> (u8, Vec<u8>) {
        assert_eq!(frame[0] & 0x80, 0x80, "FIN bit not set");
        assert_eq!(frame[1] & 0x80, 0x80, "client frame is not masked");
        let len = (frame[1] & 0x7F) as usize;
        assert!(len < 126, "test frames must be shorter than 126 bytes");
        let mask = &frame[2..6];
        let payload = frame[6..6 + len]
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ mask[i % 4])
            .collect();
        (frame[0] & 0x0F, payload)
    }

    #[test]
    fn handshake_writes_a_well_formed_upgrade_request() {
        let config = test_config(None);
        let (_sink, _stream, capture) = handshake(&config, mock_reader(&[]));
        let capture = capture.borrow();
        let request = core::str::from_utf8(&capture).expect("request is not utf-8");
        assert!(request.starts_with("GET /ocpp/CP001 HTTP/1.1\r\n"));
        for header in [
            "Host: csms.example.com\r\n",
            "Upgrade: websocket\r\n",
            "Connection: Upgrade\r\n",
            "Sec-WebSocket-Version: 13\r\n",
            "Sec-WebSocket-Protocol: ocpp1.6\r\n",
            "Sec-WebSocket-Key: ",
        ] {
            assert!(
                request.contains(header),
                "missing {header:?} in {request:?}"
            );
        }
    }

    #[test]
    fn handshake_survives_a_split_101_response() {
        // A real server need not answer in one socket read; the response must
        // accumulate until the headers complete.
        let config = test_config(None);
        let mut reader = mock_reader(&[]);
        reader.response_split = Some(32);
        handshake(&config, reader);
    }

    #[test]
    fn handshake_fails_on_eof_before_the_response() {
        let config = test_config(None);
        let mut reader = mock_reader(&[]);
        reader.respond_101 = false; // first read is already EOF
        let (result, _capture) = dial(&config, reader);
        assert!(result.is_err());
    }

    #[test]
    fn handshake_fails_when_the_response_overflows_the_buffer() {
        // Valid-looking headers that never terminate, longer than
        // `handshake_buffer_size`.
        let config = test_config(None);
        let mut reader = mock_reader(&[]);
        reader.respond_101 = false;
        let mut pad = b"X-Pad: ".to_vec();
        pad.extend_from_slice(&[b'a'; 500]);
        reader.chunks = [
            b"HTTP/1.1 101 Switching Protocols\r\n".to_vec(),
            pad.clone(),
            pad,
        ]
        .into();
        let (result, _capture) = dial(&config, reader);
        assert!(result.is_err());
    }

    #[test]
    fn a_frame_piggybacked_on_the_101_is_not_lost() {
        // The read that delivers the handshake tail may also contain the first
        // WebSocket frame — `pending` must carry it into the stream half.
        let config = test_config(None);
        let mut reader = mock_reader(&[]);
        reader.piggyback = vec![0x81, 0x02, b'h', b'i']; // FIN text frame, "hi"
        let (_sink, mut stream, _capture) = handshake(&config, reader);
        match futures::executor::block_on(stream.recv()) {
            Ok(Some(TransportEvent::Frame(text))) => assert_eq!(text, "hi"),
            other => panic!("expected Frame(\"hi\"), got {other:?}"),
        }
    }

    #[test]
    fn stream_reports_text_ping_pong_then_eof() {
        let config = test_config(None);
        let reader = mock_reader(&[
            &[0x81, 0x05, b'h', b'e', b'l', b'l', b'o'],
            &[0x89, 0x03, b'p', b'i', b'n'],
            &[0x8A, 0x04, b'p', b'o', b'n', b'g'],
        ]);
        let (_sink, mut stream, _capture) = handshake(&config, reader);

        match futures::executor::block_on(stream.recv()) {
            Ok(Some(TransportEvent::Frame(text))) => assert_eq!(text, "hello"),
            other => panic!("expected Frame(\"hello\"), got {other:?}"),
        }
        match futures::executor::block_on(stream.recv()) {
            Ok(Some(TransportEvent::Ping(payload))) => assert_eq!(payload, b"pin"),
            other => panic!("expected Ping, got {other:?}"),
        }
        match futures::executor::block_on(stream.recv()) {
            Ok(Some(TransportEvent::Pong(payload))) => assert_eq!(payload, b"pong"),
            other => panic!("expected Pong, got {other:?}"),
        }
        // Scripted input exhausted → clean EOF.
        assert!(matches!(
            futures::executor::block_on(stream.recv()),
            Ok(None)
        ));
    }

    #[test]
    fn send_writes_a_masked_text_frame() {
        let config = test_config(None);
        let (mut sink, _stream, capture) = handshake(&config, mock_reader(&[]));
        let request_len = capture.borrow().len();
        futures::executor::block_on(sink.send("hi".to_string())).expect("send failed");
        let capture = capture.borrow();
        let (opcode, payload) = decode_client_frame(&capture[request_len..]);
        assert_eq!(opcode, 0x1, "expected a text frame");
        assert_eq!(payload, b"hi");
    }

    #[test]
    fn pong_echoes_the_payload_verbatim() {
        let config = test_config(None);
        let (mut sink, _stream, capture) = handshake(&config, mock_reader(&[]));
        let request_len = capture.borrow().len();
        futures::executor::block_on(sink.pong(vec![1, 2, 3, 4])).expect("pong failed");
        let capture = capture.borrow();
        let (opcode, payload) = decode_client_frame(&capture[request_len..]);
        assert_eq!(opcode, 0xA, "expected a pong frame");
        assert_eq!(payload, [1, 2, 3, 4]);
    }

    #[test]
    fn dropped_buffers_return_to_the_pool() {
        futures::executor::block_on(async {
            // Deterministic start: the pool is a global the other tests could
            // in principle have already fed.
            while BUF_POOL.lock().await.pop().is_some() {}

            let buf = PooledBuf::take(64).await;
            let ptr = buf.ptr;
            drop(buf);
            assert_eq!(BUF_POOL.lock().await.len(), 1);

            // A same-length `take` recycles the just-dropped buffer.
            let recycled = PooledBuf::take(64).await;
            assert_eq!(recycled.ptr, ptr);
            drop(recycled);

            // Other lengths pool independently.
            drop(PooledBuf::take(256).await);
            assert_eq!(BUF_POOL.lock().await.len(), 2);

            // And the pool never grows past BUF_POOL_CAP.
            let mut held = Vec::new();
            for _ in 0..BUF_POOL_CAP + 2 {
                held.push(PooledBuf::take(96).await);
            }
            drop(held);
            assert_eq!(BUF_POOL.lock().await.len(), BUF_POOL_CAP);
        });
    }
}
