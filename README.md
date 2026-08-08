# 🔌 OCPP Client

> **A lightweight, embedded-friendly Rust OCPP communication framework for building real charge points and CSMS integrations.**

[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg)](#license)
[![Crates.io](https://img.shields.io/crates/v/ocpp-client)](https://crates.io/crates/ocpp-client)
[![Documentation](https://docs.rs/ocpp-client/badge.svg)](https://docs.rs/ocpp-client)
[![.github/workflows/ci.yaml](https://github.com/flowionab/ocpp-client/actions/workflows/ci.yaml/badge.svg)](https://github.com/flowionab/ocpp-client/actions/workflows/ci.yaml)
[![no_std](https://img.shields.io/badge/no__std-compatible-brightgreen.svg)](#features)

---

## 🚀 Overview

**OCPP Client** is the communication layer of the **Flowion Rust OCPP ecosystem**, providing the networking and transport foundation required to build OCPP-enabled charge points and backend integrations.

The library handles the complexities of establishing and managing OCPP connections, including:

* Connection lifecycle management
* Transport handling
* Message routing
* Communication reliability

OCPP message types and protocol definitions are provided by [`ocpp-types`](https://github.com/flowionab/ocpp-types), while OCPP Client focuses on the communication layer required to exchange messages between charge points and Charge Station Management Systems (CSMS).

Designed for both cloud/server environments and resource-constrained embedded systems, OCPP Client speaks WebSocket out of the box and compiles for `no_std` + `alloc` targets. An `embassy-net`-based transport and an STM32 board scaffold ship alongside it as **experimental** crates - see [Supported Transports](#-supported-transports).

The library currently supports **OCPP 1.6J**, **OCPP 2.0.1**, and **OCPP 2.1**.

---

## ✨ Features

* 🦀 Native Rust implementation
* 🔌 OCPP communication layer
* ⚡ OCPP **1.6J support**
* 🚀 OCPP **2.0.1 support**
* ⚡ OCPP **2.1 support**
* 🌐 WebSocket transport
* 🔒 Secure WebSocket (WSS)
* 🔋 `embassy-net` transport for embedded targets (experimental)
* 🔄 Connection lifecycle management
* 💓 Scheduled WebSocket keepalive with dead-peer detection
* 📨 Message routing
* 🧩 Transport abstraction
* 🪶 Lightweight runtime
* 💾 `no_std` support for embedded environments
* 🖥️ `std` support enabled by default for desktop and server applications

---

## 🔌 Supported Protocols

| Protocol   | Status         | Actions wired up |
| ---------- | -------------- | ---------------- |
| OCPP 1.6J  | ✅ Supported    | all 39           |
| OCPP 2.0.1 | ✅ Supported    | all 64           |
| OCPP 2.1   | ✅ Supported    | all 91           |

1.6's 39 includes the eleven actions from the security whitepaper (`SignCertificate`, `GetLog`,
`SignedUpdateFirmware` and friends), wired up in **0.4.0** when `ocpp-types` first defined them.

Every action defined by [`ocpp-types`](https://crates.io/crates/ocpp-types) for each version has a
`send_*`/`on_*` method - `tests/action_coverage.rs` fails the build otherwise, so the table can't
drift. If a method you expect is missing, check [CHANGELOG.md](CHANGELOG.md) before filing an
issue: five actions were only wired up in **0.2.1**, so a 0.2.0 build is missing
`SecurityEventNotification` (2.0.1) and `TriggerMessage`, `SetDisplayMessage`, `GetDERControl`,
`SetDERControl`, `UpdateDynamicSchedule` (2.1).

---

## 🌐 Supported Transports

| Transport                                   | Status            |
| ------------------------------------------- | ----------------- |
| WebSocket                                    | ✅ Supported       |
| Secure WebSocket (WSS), incl. mutual TLS     | ✅ Supported       |
| `embassy-net` (embedded, `no_std` + `alloc`) | 🧪 Experimental   |

The embedded transport (`crates/ocpp-transport-embassy-net`) and the NUCLEO-H723ZG firmware
scaffold (`crates/ocpp-board-stm32h723-nucleo`) compile and fully link against the real
`thumbv7em-none-eabihf` target in CI, but **neither has been run against real hardware or a real
CSMS**, and the embedded transport has no TLS. Treat them as a starting point for a board bring-up
rather than a supported deployment path. Each crate's README states its exact status.

---

## ⚙️ Feature Flags

OCPP Client supports both standard Rust environments and embedded systems.

By default, the `std` feature is enabled:

```toml
[dependencies]
ocpp-client = "0.x"
```

For embedded targets or `no_std` environments:

```toml
[dependencies]
ocpp-client = { version = "0.x", default-features = false }
```

This lets the same OCPP communication core compile for resource-constrained devices as well as server-side applications. Embedded users supply their own `Executor`/`Timer` implementations (e.g. backed by `embassy-executor`/`embassy-time`) and a `critical-section` backend for their target.

The optional `chrono` feature adds `From`/`Into` between `ocpp_types::OcppTimestamp` - the type every `dateTime` field uses - and `chrono::DateTime`, for applications that already keep time in chrono. It is interop only; chrono never reaches the wire.

The optional `validate` feature adds spec-conformance checking - see below.

---

## ✅ Validating payloads (`validate`)

Most of the specification's limits are in the types, so a violation cannot be built: a field the
schema bounds at `maxLength: 20` is a `heapless::String<20>`. Two categories escape that - bounds
too large to store inline (certificates, CSRs, OCSP results, which are growable `String`s), and
`minItems`/`minimum`/`maximum`/`multipleOf`, which no collection or integer type expresses at all.

The `validate` feature covers exactly those:

```toml
[dependencies]
ocpp-client = { version = "0.x", features = ["validate"] }
```

**Nothing calls it for you.** Validation is not wired into `Client::call`, deliberately: it would
need a `Validate` bound on `Action::Request`, and a trait bound that appears only when a feature is
enabled is not additive - one crate in your dependency graph turning it on would break an unrelated
crate's custom `Action` implementation. So you validate where you want it, which is one line:

```rust,ignore
use ocpp_client::ocpp_types::validate::Validate;

request.validate()?;                       // names the offending field, before it hits the wire
let response = client.call::<Reset>(request).await?;
```

The payoff is sharper on the receiving side. A schema violation the peer catches comes back as a
`CALLERROR` you cannot correlate to any field; validating locally gives you the JSON path. And
because a `ValidationError` converts straight into your version's error type, a handler can reject
a bad payload with the correct wire code:

```rust,ignore
client.on_clear_variable_monitoring(|request, _client| async move {
    request.validate()?;                   // -> Occurrence/PropertyConstraintViolation
    Ok(handle(request))
}).await;
```

That conversion is why the feature exists here rather than only upstream: `ocpp-types` classifies a
violation but leaves the wire code to "your version", and the versions disagree - OCPP 1.6J spells
it `OccurenceConstraintViolation`, with one `r`, where 2.0.1 and 2.1 spell it `Occurrence`. The
`From<ValidationError>` impls on `OCPP1_6Error`/`OCPP2_0_1Error`/`OCPP2_1Error` get that right, and
put the JSON path in `errorDetails` so a peer can match on it without parsing prose.

Out of scope, because the schemas do not state them: cross-field rules from the specification's
prose, your own `customData` payload, and 2.x's deliberately untyped `DataTransfer.data`.

---

## 🏗️ Architecture

OCPP Client separates protocol definitions from communication.

```text
┌────────────────────────────────────────────┐
│          Your Application                  │
│          Charge Point / CSMS Logic         │
└──────────────────────┬─────────────────────┘
                       │
                       │
┌──────────────────────▼─────────────────────┐
│              ocpp-charge-point             │
│                                            │
│  Complete charge point firmware framework  │
│  Add hardware bindings and deploy          │
└──────────────────────┬─────────────────────┘
                       │
                       │
┌──────────────────────▼─────────────────────┐
│              ocpp-client                   │
│                                            │
│  OCPP communication runtime                │
│  Transport abstraction                     │
│  WebSocket / embedded transports           │
└──────────────────────┬─────────────────────┘
                       │
                       │
┌──────────────────────▼─────────────────────┐
│               ocpp-types                   │
│                                            │
│  OCPP message types                        │
│  Protocol models                            │
│  Serialization                              │
└────────────────────────────────────────────┘
```

---

## 🌐 Flowion OCPP Ecosystem

OCPP Client is designed as a modular building block within the **Flowion Rust OCPP ecosystem**.

Each project has a focused responsibility, allowing developers to choose the right level of abstraction for their application.

---

### 📦 ocpp-types

**OCPP protocol definitions and data models**

[`ocpp-types`](https://github.com/flowionab/ocpp-types) provides the foundation for working with OCPP messages in Rust.

It contains:

* OCPP message types
* Protocol models
* Serialization and deserialization
* Version-specific protocol definitions

OCPP Client builds on top of `ocpp-types` to provide communication capabilities.

---

### 🔌 ocpp-client

**OCPP communication and transport layer**

This repository provides the runtime required to connect OCPP-enabled systems.

It handles:

* Connection management
* Transport abstraction
* Message routing
* WebSocket communication
* Embedded-compatible transports
* `no_std` environments

It is designed to run in both:

* 🖥️ Server environments
* 🔋 Embedded charge point environments

---

### ⚡ ocpp-charge-point

**Complete charge point firmware framework**

[`ocpp-charge-point`](https://github.com/flowionab/ocpp-charge-point) provides a complete framework for building OCPP-enabled charge point firmware.

The goal is to make developing custom charging hardware as simple as implementing the required hardware bindings.

Developers provide hardware-specific implementations such as:

* GPIO control
* Contactor control
* Metering interfaces
* Connector handling
* LEDs and user interfaces
* Hardware drivers

while the framework handles:

* Charge point state management
* OCPP communication
* Charging workflows
* Backend communication
* Protocol integration

This allows manufacturers and developers to build custom OCPP-compatible chargers without implementing the complete protocol stack from scratch.

---

## 🎯 Use Cases

OCPP Client can be used for:

* 🚗 Building EV charge point firmware
* 🏭 Developing OCPP-enabled hardware
* 🖥️ Building CSMS integrations
* 🧪 Testing OCPP implementations
* 🔋 Connecting embedded devices to charging platforms
* ⚡ Creating custom charging solutions
* 🤖 Automated integration testing

---

## 📦 Installation

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
ocpp-client = "0.x"
```

---

## 🚀 Quick Example

```rust
use ocpp_client::connect_1_6;
use ocpp_client::ocpp_types::v16::HeartbeatRequest;

#[tokio::main]
async fn main() {
    // `None` takes the defaults: 5s request timeout, automatic reconnect, and keepalive
    // pinging every 60s.
    let client = connect_1_6("wss://example.com/ocpp", None).await.unwrap();

    let response = client.send_heartbeat(HeartbeatRequest {}).await.unwrap();
    println!("CSMS time: {}", response.current_time);
}
```

Use `connect_2_0_1`/`connect_2_1` for those versions, or `connect` to negotiate whichever version
the server picks.

### Vendor extensions (`customData`)

2.0.1 and 2.1 hang an optional `customData` object on nearly every message. The `send_*`/`on_*`
methods use the specification's own shape - a bare `vendorId` - which is all most deployments
need. To carry your own, name the type on the action marker and go through `call`/`on`:

```rust,ignore
#[derive(serde::Serialize, serde::Deserialize)]
struct AcmeExtension {
    #[serde(rename = "vendorId")]
    vendor_id: String,
    #[serde(rename = "siteId")]
    site_id: u32,
}

let response = client.call::<Reset<AcmeExtension>>(request).await?;
```

`NoCustomData` is the other end of the trade: it accepts whatever a peer sends and discards it,
costing one byte per node instead of the field's full width - worth naming on an MCU.

---

## 💓 Keepalive & `WebSocketPingInterval`

By default a client pings the CSMS every 60 seconds and, after two unanswered pings, drops the
connection and redials. Without this a half-open link - a dropped NAT entry, a mobile connection
that vanished without a FIN - is undetectable: the socket accepts writes and nothing ever comes
back, and reconnect can't help because nothing reports the connection as closed.

```rust
use ocpp_client::{ConnectOptions, KeepaliveBehavior, KeepalivePolicy, connect_1_6};
use std::time::Duration;

let options = ConnectOptions {
    keepalive: KeepaliveBehavior::Enabled(KeepalivePolicy {
        interval: Duration::from_secs(30),
        timeout: None,          // fall back to the client's request timeout
        max_missed: 2,
    }),
    ..Default::default()
};
let client = connect_1_6("wss://example.com/ocpp", Some(options)).await?;
```

Set `keepalive: KeepaliveBehavior::Disabled` if the CSMS pings the charge point instead, or if the
deployment forbids unsolicited traffic.

This crate does not implement a device model, but it owns the ping timer, so it exposes the value
for the layer that does:

| OCPP | Variable / key | Read | Write |
| ---- | -------------- | ---- | ----- |
| 2.0.1 / 2.1 | `OCPPCommCtrlr.WebSocketPingInterval` (`GetVariables`/`SetVariables`) | `client.ping_interval()` | `client.set_ping_interval(..)` |
| 1.6 (security whitepaper) | `WebSocketPingInterval` (`GetConfiguration`/`ChangeConfiguration`) | `client.ping_interval()` | `client.set_ping_interval(..)` |

Both are non-`async`, so a `GetVariables` handler can call them directly. `None` maps to the
spec's `0` (disabled) in both directions, writes take effect immediately rather than after the
current interval finishes, and a write can enable pinging on a client that started with keepalive
disabled.

---

## 🧪 Testing

OCPP Client is designed for:

* Integration testing
* Charge point development
* Embedded testing
* CSMS validation
* Automated test environments

It can be combined with simulators and real charging hardware to validate complete OCPP workflows.

---

## 🛣️ Roadmap

Planned improvements:

* 🔌 Additional embedded transports
* 📚 More examples
* 🧪 Expanded integration tests
* 🔧 Improved developer tooling

---

## 🤝 Contributing

Contributions are welcome!

You can help by:

* 🐛 Reporting issues
* 💡 Suggesting improvements
* 📝 Improving documentation
* 🔧 Submitting pull requests

---

## 📄 License

OCPP Client is dual licensed:

* MIT License
* Apache License 2.0

You may choose either license.

---

## 🏢 About Flowion

**OCPP Client** is developed by **Flowion AB** as part of our effort to make EV charging development more accessible through modern, open-source tooling.

Flowion builds software solutions for electric vehicle charging using open standards such as **OCPP**, helping developers and businesses build reliable and scalable charging infrastructure.

---

## ⭐ Support the Project

If you find this library useful:

* ⭐ Star the repository
* 🐛 Report issues
* 💡 Suggest improvements
* 🤝 Contribute

Together we can make EV charging development easier and more accessible.