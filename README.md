# 🔌 OCPP Client

> **A lightweight, embedded-friendly Rust OCPP communication framework for building real charge points and CSMS integrations.**

[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg)](#license)
[![Crates.io](https://img.shields.io/crates/v/ocpp-client)](https://crates.io/crates/ocpp-client)
[![Documentation](https://docs.rs/ocpp-client/badge.svg)](https://docs.rs/ocpp-client)
[![.github/workflows/ci.yaml](https://github.com/flowionab/ocpp-client/actions/workflows/ci.yaml/badge.svg)](https://github.com/flowionab/ocpp-client/actions/workflows/ci.yaml)

---

## 🚀 Overview

**OCPP Client** is the communication layer of the **Flowion Rust OCPP ecosystem**, providing the networking and transport foundation required to build OCPP-enabled charge points and backend integrations.

The library handles the complexities of establishing and managing OCPP connections, including:

* Connection lifecycle management
* Transport handling
* Message routing
* Communication reliability

OCPP message types and protocol definitions are provided by [`rust-ocpp`](https://github.com/flowionab/rust-ocpp), while OCPP Client focuses on the communication layer required to exchange messages between charge points and Charge Station Management Systems (CSMS).

Designed for both cloud/server environments and resource-constrained embedded systems, OCPP Client supports standard WebSocket connections as well as `no_std` compatible transports for STM32-based platforms.

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
* 🔋 STM32-compatible transport layer
* 🔄 Connection lifecycle management
* 📨 Message routing
* 🧩 Transport abstraction
* 🪶 Lightweight runtime
* 💾 `no_std` support for embedded environments
* 🖥️ `std` support enabled by default for desktop and server applications

---

## 🔌 Supported Protocols

| Protocol   | Status         |
| ---------- | -------------- |
| OCPP 1.6J  | ✅ Supported    |
| OCPP 2.0.1 | ✅ Supported    |
| OCPP 2.1   | ✅ Supported    |

---

## 🌐 Supported Transports

| Transport              | Status      |
| ---------------------- | ----------- |
| WebSocket              | ✅ Supported |
| Secure WebSocket (WSS) | ✅ Supported |
| STM32 transport        | ✅ Supported |

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

This allows the same OCPP communication foundation to run on resource-constrained devices such as STM32 microcontrollers while still supporting traditional server-side applications.

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
│               rust-ocpp                    │
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

### 📦 rust-ocpp

**OCPP protocol definitions and data models**

[`rust-ocpp`](https://github.com/flowionab/rust-ocpp) provides the foundation for working with OCPP messages in Rust.

It contains:

* OCPP message types
* Protocol models
* Serialization and deserialization
* Version-specific protocol definitions

OCPP Client builds on top of `rust-ocpp` to provide communication capabilities.

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
use ocpp_client::Client;

#[tokio::main]
async fn main() {
    let client = Client::new();

    client
        .connect("wss://example.com/ocpp")
        .await
        .unwrap();
}
```

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