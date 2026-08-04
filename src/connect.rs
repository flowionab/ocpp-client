use crate::reconnect::{ReconnectBehavior, ReconnectPolicy, Reconnector};
use crate::transport::{TransportError, TransportSink, TransportStream};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use core::future::Future;
use core::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, SEC_WEBSOCKET_PROTOCOL};
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream, client_async_tls_with_config};
use url::Url;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct ConnectOptions<'a> {
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
    pub timeout: Option<Duration>,
    /// Whether the returned client should reconnect automatically when the WebSocket
    /// connection drops. Defaults to `ReconnectBehavior::Enabled(ReconnectPolicy::default())` -
    /// set this to `ReconnectBehavior::Disabled` to get the old one-shot-connection behavior.
    pub reconnect: ReconnectBehavior,
    /// Custom TLS trust config for `wss://` addresses. `None` (the default) uses
    /// `tokio-tungstenite`'s built-in `rustls-tls-webpki-roots` default, which only trusts
    /// public CAs - it cannot validate a CSMS certificate issued by a private/internal CA.
    /// Build a `rustls::ClientConfig` with a `RootCertStore` containing that CA's certificate
    /// (and optionally client-cert auth for mTLS) and set it here to connect to such a CSMS.
    /// `ocpp_client::rustls` re-exports the exact `rustls` version this crate was built
    /// against, so the `ClientConfig` you build is guaranteed compatible. Reconnect attempts
    /// (see `reconnect` above) reuse the same config.
    pub tls_config: Option<Arc<rustls::ClientConfig>>,
}

/// A `Client` for whichever OCPP version the server actually picked when connecting via
/// [`connect`]. Which variants exist depends on which `ocpp_1_6`/`ocpp_2_0_1`/`ocpp_2_1` features
/// are enabled, same as the version-specific `connect_*` functions.
pub enum NegotiatedClient {
    #[cfg(feature = "ocpp_1_6")]
    V1_6(crate::ocpp_1_6::OCPP1_6Client),
    #[cfg(feature = "ocpp_2_0_1")]
    V2_0_1(crate::ocpp_2_0_1::OCPP2_0_1Client),
    #[cfg(feature = "ocpp_2_1")]
    V2_1(crate::ocpp_2_1::OCPP2_1Client),
}

/// An OCPP version `connect` can offer/accept. Only variants for features enabled in this
/// build exist, same as `NegotiatedClient`'s variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcppVersion {
    #[cfg(feature = "ocpp_1_6")]
    V1_6,
    #[cfg(feature = "ocpp_2_0_1")]
    V2_0_1,
    #[cfg(feature = "ocpp_2_1")]
    V2_1,
}

impl OcppVersion {
    fn protocol(self) -> &'static str {
        match self {
            #[cfg(feature = "ocpp_1_6")]
            OcppVersion::V1_6 => "ocpp1.6",
            #[cfg(feature = "ocpp_2_0_1")]
            OcppVersion::V2_0_1 => "ocpp2.0.1",
            #[cfg(feature = "ocpp_2_1")]
            OcppVersion::V2_1 => "ocpp2.1",
        }
    }

    /// Every version compiled into this build, newest first - `connect`'s default set of
    /// versions to offer when the caller doesn't restrict it via the `versions` argument.
    #[allow(clippy::vec_init_then_push)]
    fn all_compiled_in() -> Vec<OcppVersion> {
        let mut versions = Vec::new();
        #[cfg(feature = "ocpp_2_1")]
        versions.push(OcppVersion::V2_1);
        #[cfg(feature = "ocpp_2_0_1")]
        versions.push(OcppVersion::V2_0_1);
        #[cfg(feature = "ocpp_1_6")]
        versions.push(OcppVersion::V1_6);
        versions
    }
}

/// Connect to an OCPP server over WebSocket, offering the given `versions` (or, if `None`,
/// every version compiled into this crate via its `ocpp_1_6`/`ocpp_2_0_1`/`ocpp_2_1` features)
/// in the `Sec-WebSocket-Protocol` header, and using whichever one the server picks - rather
/// than requiring the caller to already know the server's supported version like
/// `connect_1_6`/`connect_2_0_1`/`connect_2_1` do. `versions` also controls preference order
/// (offered in the slice's order); the choice among the offered set is entirely the server's
/// per RFC 6455.
pub async fn connect(
    address: &str,
    versions: Option<&[OcppVersion]>,
    options: Option<ConnectOptions<'_>>,
) -> Result<NegotiatedClient, Box<dyn std::error::Error + Send + Sync>> {
    let all_compiled_in;
    let versions = match versions {
        Some(versions) => versions,
        None => {
            all_compiled_in = OcppVersion::all_compiled_in();
            &all_compiled_in
        }
    };
    let offered = versions
        .iter()
        .map(|v| v.protocol())
        .collect::<Vec<_>>()
        .join(", ");
    let (stream, negotiated) = setup_socket(address, &offered, options.clone()).await?;
    let protocol = versions
        .iter()
        .find(|v| v.protocol() == negotiated)
        .map(|v| v.protocol())
        .ok_or_else(|| format!("Server negotiated unsupported protocol: {negotiated}"))?;
    let (timeout, reconnector, policy) = prepare(address, protocol, options);
    let (sink, source) = crate::transport::websocket::split(stream);

    Ok(match protocol {
        #[cfg(feature = "ocpp_1_6")]
        "ocpp1.6" => NegotiatedClient::V1_6(crate::Client::from_transport_with_reconnect(
            sink,
            source,
            timeout,
            Box::new(crate::runtime::tokio::TokioExecutor),
            Box::new(crate::runtime::tokio::TokioTimer),
            reconnector,
            policy,
        )),
        #[cfg(feature = "ocpp_2_0_1")]
        "ocpp2.0.1" => NegotiatedClient::V2_0_1(crate::Client::from_transport_with_reconnect(
            sink,
            source,
            timeout,
            Box::new(crate::runtime::tokio::TokioExecutor),
            Box::new(crate::runtime::tokio::TokioTimer),
            reconnector,
            policy,
        )),
        #[cfg(feature = "ocpp_2_1")]
        "ocpp2.1" => NegotiatedClient::V2_1(crate::Client::from_transport_with_reconnect(
            sink,
            source,
            timeout,
            Box::new(crate::runtime::tokio::TokioExecutor),
            Box::new(crate::runtime::tokio::TokioTimer),
            reconnector,
            policy,
        )),
        _ => unreachable!("protocol only ever holds a value returned by OcppVersion::protocol"),
    })
}

/// Connect to an OCPP 1.6 server over WebSocket.
#[cfg(feature = "ocpp_1_6")]
pub async fn connect_1_6(
    address: &str,
    options: Option<ConnectOptions<'_>>,
) -> Result<crate::ocpp_1_6::OCPP1_6Client, Box<dyn std::error::Error + Send + Sync>> {
    let (timeout, reconnector, policy) = prepare(address, "ocpp1.6", options.clone());
    let (stream, _protocol) = setup_socket(address, "ocpp1.6", options).await?;
    let (sink, source) = crate::transport::websocket::split(stream);
    Ok(crate::Client::from_transport_with_reconnect(
        sink,
        source,
        timeout,
        Box::new(crate::runtime::tokio::TokioExecutor),
        Box::new(crate::runtime::tokio::TokioTimer),
        reconnector,
        policy,
    ))
}

/// Connect to an OCPP 2.0.1 server over WebSocket.
#[cfg(feature = "ocpp_2_0_1")]
pub async fn connect_2_0_1(
    address: &str,
    options: Option<ConnectOptions<'_>>,
) -> Result<crate::ocpp_2_0_1::OCPP2_0_1Client, Box<dyn std::error::Error + Send + Sync>> {
    let (timeout, reconnector, policy) = prepare(address, "ocpp2.0.1", options.clone());
    let (stream, _protocol) = setup_socket(address, "ocpp2.0.1", options).await?;
    let (sink, source) = crate::transport::websocket::split(stream);
    Ok(crate::Client::from_transport_with_reconnect(
        sink,
        source,
        timeout,
        Box::new(crate::runtime::tokio::TokioExecutor),
        Box::new(crate::runtime::tokio::TokioTimer),
        reconnector,
        policy,
    ))
}

/// Connect to an OCPP 2.1 server over WebSocket.
#[cfg(feature = "ocpp_2_1")]
pub async fn connect_2_1(
    address: &str,
    options: Option<ConnectOptions<'_>>,
) -> Result<crate::ocpp_2_1::OCPP2_1Client, Box<dyn std::error::Error + Send + Sync>> {
    let (timeout, reconnector, policy) = prepare(address, "ocpp2.1", options.clone());
    let (stream, _protocol) = setup_socket(address, "ocpp2.1", options).await?;
    let (sink, source) = crate::transport::websocket::split(stream);
    Ok(crate::Client::from_transport_with_reconnect(
        sink,
        source,
        timeout,
        Box::new(crate::runtime::tokio::TokioExecutor),
        Box::new(crate::runtime::tokio::TokioTimer),
        reconnector,
        policy,
    ))
}

/// Pulls the timeout/reconnect settings out of `options` and, if reconnect is enabled, builds
/// the `Reconnector` that redials this same address/protocol/credentials. Shared by all three
/// `connect_*` entry points.
fn prepare(
    address: &str,
    protocol: &'static str,
    options: Option<ConnectOptions<'_>>,
) -> (Duration, Option<Box<dyn Reconnector>>, ReconnectPolicy) {
    let timeout = options
        .as_ref()
        .and_then(|o| o.timeout)
        .unwrap_or(DEFAULT_TIMEOUT);
    let reconnect = options.as_ref().map(|o| o.reconnect).unwrap_or_default();
    let username = options
        .as_ref()
        .and_then(|o| o.username)
        .map(str::to_string);
    let password = options
        .as_ref()
        .and_then(|o| o.password)
        .map(str::to_string);
    let tls_config = options.as_ref().and_then(|o| o.tls_config.clone());

    match reconnect {
        ReconnectBehavior::Disabled => (timeout, None, ReconnectPolicy::default()),
        ReconnectBehavior::Enabled(policy) => {
            let reconnector: Box<dyn Reconnector> = Box::new(WebSocketReconnector {
                address: address.to_string(),
                protocol,
                username,
                password,
                tls_config,
            });
            (timeout, Some(reconnector), policy)
        }
    }
}

/// Redials `address` with the original protocol/credentials/TLS config whenever `Client`'s
/// background read loop needs a fresh transport after a disconnect.
struct WebSocketReconnector {
    address: String,
    protocol: &'static str,
    username: Option<String>,
    password: Option<String>,
    tls_config: Option<Arc<rustls::ClientConfig>>,
}

impl Reconnector for WebSocketReconnector {
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
            let options = ConnectOptions {
                username: self.username.as_deref(),
                password: self.password.as_deref(),
                timeout: None,
                reconnect: ReconnectBehavior::Disabled,
                tls_config: self.tls_config.clone(),
            };
            let (stream, _protocol) =
                setup_socket(&self.address, self.protocol, Some(options)).await?;
            Ok(crate::transport::websocket::split(stream))
        })
    }
}

async fn setup_socket(
    address: &str,
    protocols: &str,
    options: Option<ConnectOptions<'_>>,
) -> Result<
    (WebSocketStream<MaybeTlsStream<TcpStream>>, String),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let address = Url::parse(address)?;

    let socket_addrs = address.socket_addrs(|| None)?;
    let stream = TcpStream::connect(&*socket_addrs).await?;

    let mut request: Request<()> = address.to_string().into_client_request()?;
    request
        .headers_mut()
        .insert(SEC_WEBSOCKET_PROTOCOL, protocols.parse()?);
    let mut tls_config = None;
    if let Some(options) = options {
        if let Some(username) = options.username {
            let data = format!("{}:{}", username, options.password.unwrap_or(""));
            let encoded = BASE64_STANDARD.encode(data);
            request
                .headers_mut()
                .insert(AUTHORIZATION, format!("Basic {encoded}").parse()?);
        }
        tls_config = options.tls_config;
    }

    let connector = tls_config.map(Connector::Rustls);
    let (stream, response) = client_async_tls_with_config(request, stream, None, connector).await?;

    let protocol = response
        .headers()
        .get(SEC_WEBSOCKET_PROTOCOL)
        .ok_or("No OCPP protocol negotiated")?;

    Ok((stream, protocol.to_str()?.to_string()))
}
