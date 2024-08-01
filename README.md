# OCPP Client

[![Crates.io](https://img.shields.io/crates/v/ocpp-client)](https://crates.io/crates/ocpp-client)
[![Documentation](https://docs.rs/ocpp-client/badge.svg)](https://docs.rs/ocpp-client)
[![Build Status](https://github.com/flowionab/ocpp-client/workflows/ci.yaml/badge.svg)](https://github.com/flowionab/ocpp-client/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

`ocpp-client` is a Rust library that provides an OCPP (Open Charge Point Protocol) client implementation. This library enables developers to integrate with central system (CSMS) that use the OCPP protocol, allowing for seamless communication and efficient.

## Features

- **OCPP 1.6 JSON support**: Communicate with OCPP 1.6 JSON compliant servers (CSMS).
- **Async/Await support**: Built using asynchronous Rust for high performance and scalability.
- **Customizable**: Easily extendable to add custom message handlers or to support additional OCPP features.
- **Error Handling**: Robust error handling and logging to assist in debugging and maintenance.
- **Comprehensive Documentation**: Detailed documentation with examples to get you started quickly.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
ocpp-client = "0.1"
```

## Usage

Here's a simple example to get you started:

```rust
use ocpp_client::{Client, Configuration};

#[tokio::main]
async fn main() {
    
}
```

## Documentation

The full documentation is available on [docs.rs](https://docs.rs/ocpp-client).

## Examples

Check out the [examples](https://github.com/yourusername/ocpp-client/tree/main/examples) directory for more usage examples.

## Contributing

Contributions are welcome! Please see the [CONTRIBUTING.md](https://github.com/yourusername/ocpp-client/blob/main/CONTRIBUTING.md) for more details.

## License

This project is licensed under the MIT License. See the [LICENSE](https://github.com/yourusername/ocpp-client/blob/main/LICENSE) file for details.