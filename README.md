# OCPP Client

[![Crates.io](https://img.shields.io/crates/v/ocpp-client)](https://crates.io/crates/ocpp-client)
[![Documentation](https://docs.rs/ocpp-client/badge.svg)](https://docs.rs/ocpp-client)
[![.github/workflows/ci.yaml](https://github.com/flowionab/ocpp-client/actions/workflows/ci.yaml/badge.svg)](https://github.com/flowionab/ocpp-client/actions/workflows/ci.yaml)

## Overview

`ocpp-client` is a Rust library that provides an OCPP (Open Charge Point Protocol) client implementation. This library enables developers to integrate with central system (CSMS) that use the OCPP protocol, allowing for seamless communication and efficient.

## Features

- **OCPP 1.6 and 2.0.1 support**: Communicate with OCPP 1.6 or 2.0.1 JSON compliant servers (CSMS).
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
use ocpp_client::connect;

#[tokio::main]
async fn main() {
    let client = connect("wss://my-csms.com/CHARGER_IDENTITY", None).await?;

    match client {
        OCPP1_6(client) => {
            // Do 1.6 specific operations
        },
        OCPP2_0_1(client) => {
            // Do 2.0.1 specific operations
        },
    }
}
```

## Documentation

The full documentation is available on [docs.rs](https://docs.rs/ocpp-client).

## Examples

Check out the [examples](https://github.com/flowionab/ocpp-client/tree/main/examples) directory for more usage examples.

## Contributing

Contributions are welcome! Please see the [CONTRIBUTING.md](https://github.com/flowionab/ocpp-client/blob/main/CONTRIBUTING.md) for more details.

## License

This project is licensed under the MIT License. See the [LICENSE](https://github.com/flowionab/ocpp-client/blob/main/LICENSE) file for details.