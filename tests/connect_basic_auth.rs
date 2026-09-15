//! The HTTP Basic credential a charge point presents is **bytes**, not text.
//!
//! OCPP 1.6J Security Profile 1/2 (and 2.x's `BasicAuthPassword`) configure the password as the
//! hexadecimal representation of an authorization key, and the charge point sends the *decoded*
//! bytes - 20 of them for a 1.6 `AuthorizationKey`. Random bytes are almost never valid UTF-8, so
//! a `&str` password cannot carry them at all: a caller could only ever send the hex text, which a
//! spec-following CSMS then rejects on every connect, while both halves of the configuration look
//! right to a human.
#![allow(clippy::result_large_err)]
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use ocpp_client::{ConnectOptions, ReconnectBehavior, connect_1_6};
use tokio::net::TcpListener;

#[tokio::test]
async fn sends_a_non_utf8_password_byte_for_byte() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut authorization = None;
        let _ws = tokio_tungstenite::accept_hdr_async(
            tcp,
            |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
             mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                authorization = req
                    .headers()
                    .get("Authorization")
                    .map(|v| v.to_str().unwrap().to_string());
                response
                    .headers_mut()
                    .insert("Sec-WebSocket-Protocol", "ocpp1.6".parse().unwrap());
                Ok(response)
            },
        )
        .await
        .unwrap();
        authorization
    });

    // Not valid UTF-8 -- 0xff and 0xfe never appear in it -- which is the whole point. (No runtime
    // `from_utf8` check: rustc's `invalid_from_utf8` lint already proves it at compile time.)
    let key: [u8; 20] = [
        0xff, 0x00, 0x01, 0xfe, 0x80, 0x7f, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x90, 0xa0,
        0xb0, 0xc0, 0xd0, 0xe0, 0xf0,
    ];

    let _client = connect_1_6(
        &format!("ws://{addr}"),
        Some(ConnectOptions {
            username: Some("CP-1"),
            password: Some(&key),
            reconnect: ReconnectBehavior::Disabled,
            ..ConnectOptions::default()
        }),
    )
    .await
    .unwrap();

    let header = server.await.unwrap().expect("an Authorization header");
    let encoded = header.strip_prefix("Basic ").expect("the Basic scheme");
    let mut expected = b"CP-1:".to_vec();
    expected.extend_from_slice(&key);
    assert_eq!(BASE64_STANDARD.decode(encoded).unwrap(), expected);
}
