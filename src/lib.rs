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

pub use rust_ocpp;

pub use action::Action;
pub use client::Client;
pub use error::{ClientError, ProtocolError};
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

#[cfg(all(feature = "websocket", feature = "ocpp_1_6"))]
pub use connect::connect_1_6;

#[cfg(all(feature = "websocket", feature = "ocpp_2_0_1"))]
pub use connect::connect_2_0_1;

#[cfg(all(feature = "websocket", feature = "ocpp_2_1"))]
pub use connect::connect_2_1;
