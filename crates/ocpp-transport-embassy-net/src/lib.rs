//! `no_std`+`alloc` OCPP-over-WebSocket transport for [`ocpp_client`], built on `embassy-net`.
//!
//! Chip-agnostic: this crate only needs an already-configured `embassy_net::Stack` (Ethernet,
//! Wi-Fi, whatever `embassy-net` driver you're using) - it has no knowledge of any specific
//! MCU. Pair it with a board-specific crate that brings up `embassy-net`'s `Stack` for your
//! hardware (e.g. STM32H723's Ethernet MAC + an RMII PHY via `embassy-stm32`).
//!
//! # Status
//!
//! Scaffold / early days - see this crate's README for what's implemented, what's simplified,
//! and what's not been run against real hardware yet. In short: plaintext `ws://` only (no TLS
//! yet), and the WebSocket close handshake is simplified (we don't send a close-reply frame).
//!
//! # What this crate provides
//!
//! - [`transport::connect`] - dials a TCP connection via a `Stack`, performs the WebSocket
//!   opening handshake, and returns boxed `ocpp_client::TransportSink`/`TransportStream`
//!   implementations ready to hand to `ocpp_client::Client::from_transport_with_reconnect`.
//! - [`transport::EmbassyNetReconnector`] - an `ocpp_client::Reconnector` impl that redials the
//!   same remote endpoint, for `Client`'s automatic-reconnect support.
//! - [`runtime::EmbassyExecutor`] / [`runtime::EmbassyTimer`] - `ocpp_client::Executor`/`Timer`
//!   backed by `embassy-executor`/`embassy-time`.
//!
//! # What this crate does *not* provide
//!
//! - MCU/PHY bring-up (clock tree, RMII pins, PHY reset) - that's the board-specific crate's
//!   job, which then hands this crate an already-`Stack`.
//! - TLS (`wss://`) - phase 2, see the README.
//! - OCPP action definitions - that's `ocpp_client::ocpp_1_6`/`ocpp_2_0_1`/`ocpp_2_1`, used the
//!   same way regardless of transport.

#![no_std]

extern crate alloc;

pub mod runtime;
pub mod transport;

pub use runtime::{EmbassyExecutor, EmbassyTimer};
pub use transport::{ConnectConfig, EmbassyNetReconnector, RngFactory, connect};
