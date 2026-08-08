//! Proves OCPP Security Profile 3 (mutual TLS: the server requires and verifies a client
//! certificate, no HTTP Basic Auth involved) actually works through `ConnectOptions::tls_config`.
//! A `rustls::ClientConfig` built with `.with_client_auth_cert(..)` (instead of
//! `.with_no_client_auth()`) presents a client certificate during the handshake; the server is
//! configured with a `WebPkiClientVerifier` that only trusts that exact certificate. The second
//! test proves the server actually enforces this by rejecting a client with no certificate at
//! all.
#![allow(clippy::result_large_err)]
use futures::{SinkExt, StreamExt};
use ocpp_client::rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use ocpp_client::rustls::server::WebPkiClientVerifier;
use ocpp_client::rustls::{ClientConfig, RootCertStore, ServerConfig};
use ocpp_client::{ConnectOptions, connect_1_6};
use ocpp_types::OcppTimestamp;
use ocpp_types::v16::HeartbeatRequest;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;

fn self_signed(name: &str) -> (CertificateDer<'static>, PrivatePkcs8KeyDer<'static>) {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![name.to_string()]).unwrap();
    let key_der = PrivatePkcs8KeyDer::from(signing_key.serialize_der());
    (cert.der().clone(), key_der)
}

#[tokio::test]
async fn connects_over_wss_with_mutual_tls_client_certificate() {
    let (server_cert, server_key) = self_signed("127.0.0.1");
    let (client_cert, client_key) = self_signed("ocpp-client-under-test");

    let mut client_trust_root = RootCertStore::empty();
    client_trust_root.add(client_cert.clone()).unwrap();
    let client_verifier = WebPkiClientVerifier::builder(Arc::new(client_trust_root))
        .build()
        .unwrap();
    let server_config = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(vec![server_cert.clone()], server_key.into())
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

    let mut server_trust_root = RootCertStore::empty();
    server_trust_root.add(server_cert).unwrap();
    let tls_config = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(server_trust_root)
            .with_client_auth_cert(vec![client_cert], client_key.into())
            .unwrap(),
    );

    let options = ConnectOptions {
        tls_config: Some(tls_config),
        // Security Profile 3 authenticates via the client certificate above; no Basic Auth.
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
async fn rejects_a_client_with_no_certificate_when_the_server_requires_one() {
    let (server_cert, server_key) = self_signed("127.0.0.1");
    let (client_cert, _unused_key) = self_signed("some-other-client");

    let mut client_trust_root = RootCertStore::empty();
    client_trust_root.add(client_cert).unwrap();
    let client_verifier = WebPkiClientVerifier::builder(Arc::new(client_trust_root))
        .build()
        .unwrap();
    let server_config = ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(vec![server_cert.clone()], server_key.into())
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        // The server is expected to abort the TLS handshake: no client certificate was
        // presented, but `with_client_cert_verifier` requires one.
        let _ = acceptor.accept(tcp).await;
    });

    let mut server_trust_root = RootCertStore::empty();
    server_trust_root.add(server_cert).unwrap();
    let tls_config = Arc::new(
        ClientConfig::builder()
            .with_root_certificates(server_trust_root)
            .with_no_client_auth(),
    );

    let options = ConnectOptions {
        tls_config: Some(tls_config),
        ..Default::default()
    };
    let result = connect_1_6(&format!("wss://{addr}"), Some(options)).await;
    assert!(result.is_err());

    server.await.unwrap();
}
