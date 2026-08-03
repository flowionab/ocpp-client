use base64::Engine;
use base64::prelude::BASE64_STANDARD;
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
}

/// Connect to an OCPP 1.6 server over WebSocket.
#[cfg(feature = "ocpp_1_6")]
pub async fn connect_1_6(
    address: &str,
    options: Option<ConnectOptions<'_>>,
) -> Result<crate::ocpp_1_6::OCPP1_6Client, Box<dyn std::error::Error + Send + Sync>> {
    let timeout = options
        .as_ref()
        .and_then(|o| o.timeout)
        .unwrap_or(DEFAULT_TIMEOUT);
    let (stream, _protocol) = setup_socket(address, "ocpp1.6", options).await?;
    let (sink, source) = crate::transport::websocket::split(stream);
    Ok(crate::Client::from_transport(
        sink,
        source,
        timeout,
        Box::new(crate::runtime::tokio::TokioExecutor),
        Box::new(crate::runtime::tokio::TokioTimer),
    ))
}

/// Connect to an OCPP 2.0.1 server over WebSocket.
#[cfg(feature = "ocpp_2_0_1")]
pub async fn connect_2_0_1(
    address: &str,
    options: Option<ConnectOptions<'_>>,
) -> Result<crate::ocpp_2_0_1::OCPP2_0_1Client, Box<dyn std::error::Error + Send + Sync>> {
    let timeout = options
        .as_ref()
        .and_then(|o| o.timeout)
        .unwrap_or(DEFAULT_TIMEOUT);
    let (stream, _protocol) = setup_socket(address, "ocpp2.0.1", options).await?;
    let (sink, source) = crate::transport::websocket::split(stream);
    Ok(crate::Client::from_transport(
        sink,
        source,
        timeout,
        Box::new(crate::runtime::tokio::TokioExecutor),
        Box::new(crate::runtime::tokio::TokioTimer),
    ))
}

/// Connect to an OCPP 2.1 server over WebSocket.
#[cfg(feature = "ocpp_2_1")]
pub async fn connect_2_1(
    address: &str,
    options: Option<ConnectOptions<'_>>,
) -> Result<crate::ocpp_2_1::OCPP2_1Client, Box<dyn std::error::Error + Send + Sync>> {
    let timeout = options
        .as_ref()
        .and_then(|o| o.timeout)
        .unwrap_or(DEFAULT_TIMEOUT);
    let (stream, _protocol) = setup_socket(address, "ocpp2.1", options).await?;
    let (sink, source) = crate::transport::websocket::split(stream);
    Ok(crate::Client::from_transport(
        sink,
        source,
        timeout,
        Box::new(crate::runtime::tokio::TokioExecutor),
        Box::new(crate::runtime::tokio::TokioTimer),
    ))
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
    if let Some(options) = options {
        if let Some(username) = options.username {
            let data = format!("{}:{}", username, options.password.unwrap_or(""));
            let encoded = BASE64_STANDARD.encode(data);
            request
                .headers_mut()
                .insert(AUTHORIZATION, format!("Basic {encoded}").parse()?);
        }
    }

    let (stream, response) = client_async_tls(request, stream).await?;

    let protocol = response
        .headers()
        .get(SEC_WEBSOCKET_PROTOCOL)
        .ok_or("No OCPP protocol negotiated")?;

    Ok((stream, protocol.to_str()?.to_string()))
}
