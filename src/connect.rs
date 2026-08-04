use crate::reconnect::{ReconnectBehavior, ReconnectPolicy, Reconnector};
use crate::transport::{TransportError, TransportSink, TransportStream};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use core::future::Future;
use core::pin::Pin;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, SEC_WEBSOCKET_PROTOCOL};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, client_async_tls};
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

    match reconnect {
        ReconnectBehavior::Disabled => (timeout, None, ReconnectPolicy::default()),
        ReconnectBehavior::Enabled(policy) => {
            let reconnector: Box<dyn Reconnector> = Box::new(WebSocketReconnector {
                address: address.to_string(),
                protocol,
                username,
                password,
            });
            (timeout, Some(reconnector), policy)
        }
    }
}

/// Redials `address` with the original protocol/credentials whenever `Client`'s background
/// read loop needs a fresh transport after a disconnect.
struct WebSocketReconnector {
    address: String,
    protocol: &'static str,
    username: Option<String>,
    password: Option<String>,
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
    if let Some(options) = options
        && let Some(username) = options.username
    {
        let data = format!("{}:{}", username, options.password.unwrap_or(""));
        let encoded = BASE64_STANDARD.encode(data);
        request
            .headers_mut()
            .insert(AUTHORIZATION, format!("Basic {encoded}").parse()?);
    }

    let (stream, response) = client_async_tls(request, stream).await?;

    let protocol = response
        .headers()
        .get(SEC_WEBSOCKET_PROTOCOL)
        .ok_or("No OCPP protocol negotiated")?;

    Ok((stream, protocol.to_str()?.to_string()))
}
