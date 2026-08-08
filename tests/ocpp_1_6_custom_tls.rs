//! Proves `ConnectOptions::tls_config` is actually wired into the TLS handshake: the server
//! presents a self-signed certificate that only a client trusting that exact certificate as a
//! root would accept. `connect_1_6` with a custom `rustls::ClientConfig` (built from
//! `ocpp_client::rustls`, this crate's re-export) should succeed; the default (public-CA-only)
//! trust config would reject this certificate.
#![allow(clippy::result_large_err)]
use futures::{SinkExt, StreamExt};
use ocpp_client::rustls::pki_types::PrivatePkcs8KeyDer;
use ocpp_client::rustls::{ClientConfig, RootCertStore, ServerConfig};
use ocpp_client::{ConnectOptions, connect_1_6};
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::HeartbeatRequest;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn connects_over_wss_with_a_custom_root_ca() {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let cert_der = cert.der().clone();
    let key_der = PrivatePkcs8KeyDer::from(signing_key.serialize_der());

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der.clone()], key_der.into())
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let mut ws = tokio_tungstenite::accept_hdr_async(
            tls,
            |_req: &tokio_tungstenite::tungstenite::handshake::server::Request,
             mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "ocpp1.6".parse().unwrap());
                Ok(response)
            },
        )
        .await
        .unwrap();

        let frame = match ws.next().await.unwrap().unwrap() {
            Message::Text(text) => text.to_string(),
            other => panic!("expected a text frame, got {other:?}"),
        };
        let call: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(call[2], "Heartbeat");
        let message_id = call[1].as_str().unwrap().to_string();

        let response = json!([3, message_id, { "currentTime": "2024-01-01T00:00:00Z" }]);
        ws.send(Message::text(serde_json::to_string(&response).unwrap()))
            .await
            .unwrap();
    });

    let mut root_store = RootCertStore::empty();
    root_store.add(cert_der).unwrap();
    let tls_config = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    );

    let options = ConnectOptions {
        tls_config: Some(tls_config),
        ..Default::default()
    };
    let client = connect_1_6(&format!("wss://{addr}"), Some(options))
        .await
        .unwrap();
    let response = client.send_heartbeat(HeartbeatRequest {}).await.unwrap();
    assert_eq!(
        response.current_time,
        OcppTimestamp::parse_rfc3339("2024-01-01T00:00:00Z").unwrap()
    );

    server.await.unwrap();
}

#[tokio::test]
async fn rejects_a_self_signed_cert_without_a_matching_trust_config() {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()]).unwrap();
    let cert_der = cert.der().clone();
    let key_der = PrivatePkcs8KeyDer::from(signing_key.serialize_der());

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der.into())
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        // The client is expected to abort the TLS handshake before ever completing it.
        let _ = acceptor.accept(tcp).await;
    });

    // No `tls_config` - falls back to the default public-CA-only trust store, which must not
    // trust this self-signed certificate.
    let result = connect_1_6(&format!("wss://{addr}"), None).await;
    assert!(result.is_err());

    server.await.unwrap();
}
