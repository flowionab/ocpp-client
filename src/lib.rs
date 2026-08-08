//! OCPP client (charge point side) protocol implementation.
//!
//! The engine (`Client<E>`, `Action`, `Transport{Sink,Stream}`) is shared by every OCPP
//! version; `OCPP1_6Client`/`OCPP2_0_1Client` are just that engine parameterized with each
//! version's error type. See CLAUDE.md for the broader architecture/roadmap.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod action;
mod client;
mod envelope;
mod error;
mod keepalive;
mod reconnect;
pub mod runtime;
mod sync;
mod transport;

#[cfg(feature = "websocket")]
mod connect;

#[cfg(feature = "ocpp_1_6")]
pub mod ocpp_1_6;

#[cfg(feature = "ocpp_2_0_1")]
pub mod ocpp_2_0_1;

#[cfg(feature = "ocpp_2_1")]
pub mod ocpp_2_1;

/// Re-exported so callers can name request/response types (e.g.
/// `ocpp_client::ocpp_types::v16::HeartbeatRequest`) using the exact `ocpp-types` version this
/// crate was compiled against, without needing to pin a matching version in their own
/// `Cargo.toml` - same rationale as the `rustls` re-export below.
pub use ocpp_types;

pub use action::Action;
pub use client::{Client, ClientConfig};
pub use error::{ClientError, ProtocolError};
pub use keepalive::{KeepaliveBehavior, KeepalivePolicy};
pub use reconnect::{ReconnectBehavior, ReconnectPolicy, Reconnector};
pub use runtime::{Elapsed, Executor, Timer};
pub use transport::{TransportError, TransportEvent, TransportSink, TransportStream};

#[cfg(feature = "tokio-runtime")]
pub use runtime::tokio::{TokioExecutor, TokioTimer};

#[cfg(feature = "websocket")]
pub use connect::ConnectOptions;

/// Re-exported so callers can build a `rustls::ClientConfig` (e.g. with a custom root CA via
/// `ConnectOptions::tls_config`) using the exact `rustls` version this crate was compiled
/// against, without needing to pin a matching version in their own `Cargo.toml`.
#[cfg(feature = "websocket")]
pub use rustls;

#[cfg(feature = "websocket")]
pub use connect::{NegotiatedClient, OcppVersion, connect, websocket_transport};

#[cfg(all(feature = "websocket", feature = "ocpp_1_6"))]
pub use connect::connect_1_6;

#[cfg(all(feature = "websocket", feature = "ocpp_2_0_1"))]
pub use connect::connect_2_0_1;

#[cfg(all(feature = "websocket", feature = "ocpp_2_1"))]
pub use connect::connect_2_1;
