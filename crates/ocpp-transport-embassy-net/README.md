# ocpp-transport-embassy-net

`no_std`+`alloc` OCPP-over-WebSocket transport for [`ocpp-client`](../..), built on
[`embassy-net`](https://docs.embassy.dev/embassy-net). Chip-agnostic: it only needs an
already-configured `embassy_net::Stack` (Ethernet, Wi-Fi, whatever driver you're using) and has
no knowledge of any specific MCU. Pair it with a board-specific crate that brings up
`embassy-net`'s `Stack` for your hardware - this is the first piece of the STM32H723 plan
discussed in `../../PRODUCTION_READINESS.md` item 6, split out on purpose so the
STM32H723-specific bring-up (RMII pins, PHY, clock tree) lives in its own crate later.

## Status: scaffold, type-checks against real hardware, not yet run against real hardware

`cargo check --target thumbv7em-none-eabihf` passes for this crate (see "Verifying it builds"
below), and clippy is clean. It has **not** been run against a real CSMS or real embedded
hardware yet - the WebSocket handshake/framing logic is implemented carefully but the highest-risk
correctness area (buffer/remainder bookkeeping across partial reads and fragmented frames) is
exactly the part that's hardest to get right without a real peer to test against. Treat this as a
solid starting point to iterate on with a real server, not a finished implementation.

## What's implemented

- [`transport::connect`] - dials `config.remote` over TCP, performs the WebSocket opening
  handshake, and returns boxed `ocpp_client::TransportSink`/`TransportStream` halves.
- [`transport::EmbassyNetReconnector`] - an `ocpp_client::Reconnector` that redials with the same
  `ConnectConfig`, for `Client::from_transport_with_reconnect`'s automatic-reconnect support.
- [`runtime::EmbassyExecutor`] / [`runtime::EmbassyTimer`] - `ocpp_client::Executor`/`Timer`
  backed by `embassy-executor`/`embassy-time`.

## What's simplified / not implemented

- **`ws://` only, no TLS.** Production CSMS backends generally require `wss://`. Adding TLS means
  pulling in something like [`embedded-tls`](https://crates.io/crates/embedded-tls), sizing
  handshake buffers against your MCU's SRAM budget, and compiling the trusted CA in as bytes
  (there's no filesystem to load one from) - deliberately left for a follow-up so the harder
  "does the plaintext path even work on real hardware" question gets answered first.
- **No close-reply frame.** RFC6455's close handshake is two-sided (the receiver of a Close frame
  should echo one back before the TCP connection drops); this transport just ends the stream when
  it sees a Close frame, the same simplification `ocpp-client`'s own tokio-tungstenite-based
  transport effectively relies on tungstenite to paper over. Most peers tear down the TCP
  connection shortly after sending Close regardless, so this is unlikely to matter in practice,
  but it's not spec-perfect.
- **Empty-payload pong.** Matches `ocpp-client`'s own WebSocket transport
  (`src/transport/websocket.rs`) - neither echoes the triggering ping's payload today.
- **Reads/writes go through a single shared `Mutex<WebSocketClient<..>>`** for the encode/decode
  state machine (continuation-frame tracking, RNG for masking), while the actual socket I/O uses
  independent `TcpReader`/`TcpWriter` halves with no lock between them - so a concurrent
  `send()`/`recv()` won't head-of-line-block each other on socket I/O, only briefly on the (fast,
  non-blocking) encode/decode step.

## Two things worth understanding before you build on this

**1. `getrandom` has no backend on bare-metal `thumbv7em-none-eabihf` by default.**
`ocpp-client` uses `uuid`'s `v4` feature (via `Uuid::new_v4()` for OCPP-J message IDs), which
transitively depends on `getrandom`. `getrandom` auto-detects a backend per `target_os` - which
works fine when *checking* this crate for your desktop's target (macOS/Linux/Windows all have a
real OS-provided entropy source), but a bare-metal target has no OS, so the crate hits
`compile_error!("target is not supported...")` unless you opt into the "custom backend"
mechanism. This is a whole-binary build configuration, not something a library crate's
`Cargo.toml` can fix on your behalf - your **firmware binary** needs:

```toml
# .cargo/config.toml, in the firmware binary's crate (not this library)
[build]
rustflags = ["--cfg", "getrandom_backend=\"custom\""]
```

plus a `#[no_mangle]` extern function implementing the actual randomness source (a hardware RNG
peripheral, typically) per
[getrandom's custom-backend docs](https://docs.rs/getrandom/latest/getrandom/#custom-backend).
Verifying this crate with `cargo check`/`clippy` (no linking) doesn't need the real backend
function, only the `--cfg` flag - see "Verifying it builds" below. A real firmware `.bin` will
fail to *link* without one.

**2. `embassy_executor::Spawner` and `embassy_net::Stack` are `!Sync` (and `Spawner` is `!Send`
too) - deliberately.** Embassy's execution model is a single, non-preemptive executor per core;
there's no real concurrent access for these types to guard against, so they just don't bother
implementing the `Send`/`Sync` auto-traits. `ocpp_client::Executor`/`Reconnector` were both
written as `Send + Sync + 'static` with a multi-threaded runtime (tokio) in mind, and
`TransportSink`/`TransportStream` as `Send`, with no escape hatch yet for "single-core,
cooperative, nothing is ever truly concurrent" targets. This crate bridges that gap with a small
number of `unsafe impl Send`/`Sync` assertions (`runtime::AssertSendSync`,
`transport::AssertSendSync`, `transport::AssertSendFuture`, plus direct `unsafe impl Send` on the
sink/stream structs), each with a doc comment explaining exactly why it's sound *under embassy's
normal single-core cooperative usage model* - and explicitly not sound if you're running one
executor per core and sharing values across them, or doing anything with genuine preemption.
**Read those doc comments before reusing this pattern elsewhere** - they're the most
safety-relevant code in this crate.

## Verifying it builds

```sh
rustup target add thumbv7em-none-eabihf
RUSTFLAGS='--cfg getrandom_backend="custom"' cargo check -p ocpp-transport-embassy-net --target thumbv7em-none-eabihf
RUSTFLAGS='--cfg getrandom_backend="custom"' cargo clippy -p ocpp-transport-embassy-net --target thumbv7em-none-eabihf -- -D warnings
```

The `RUSTFLAGS` override reproduces the `--cfg getrandom_backend="custom"` a real firmware binary
would set in its own `.cargo/config.toml` (see point 1 above) - without it, `cargo check` fails
on `getrandom`'s `compile_error!` before it ever reaches this crate's own code.

## Next steps

1. A board-specific crate for STM32H723: Ethernet MAC + PHY (e.g. LAN8742) bring-up via
   `embassy-stm32`, clock tree, and wiring the resulting `Stack` into `ConnectConfig`.
2. Hardware-in-the-loop testing against a real CSMS (or a simulator) - the buffer/fragmentation
   handling in `transport.rs` is the part most likely to need iteration once it meets a real
   server.
3. TLS (`wss://`), once (1) and (2) are solid.
