# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

`ocpp-client` is a Rust crate implementing the **charge point** (client) side of OCPP — the network/protocol
layer only, not charging logic or hardware control. It's mid-rewrite (`1.0.0-alpha.1`) from a previous
per-version, tokio-hardwired design (see git history before this version bump) to a single generic engine
shared by every OCPP version, with a transport abstraction so WebSocket isn't the only option long-term.

## Status: 1.6 + 2.0.1 + 2.1, WebSocket only, no_std+alloc capable

- **OCPP 1.6, 2.0.1, and 2.1** are all implemented and tested end to end, all three in `default` features,
  every action wired up including `NotifyReport` for 2.1 (see the `ocpp-types` bullet below - this was
  missing until the migration documented in `MIGRATION_OCPP_TYPES.md` closed the gap). 2.1 mirrors
  `src/ocpp_2_0_1/` exactly (`src/ocpp_2_1/{mod,error,actions}.rs`, `OCPP2_1Client`, `connect_2_1()`) -
  `OCPP2_1Error` reuses the same RPC framework error codes as `OCPP2_0_1Error` (OCPP-J's envelope/error-code
  set didn't change between 2.0.1 and 2.1). One 2.1 action, `NotifyPeriodicEventStream`, is modeled as a
  call/response pair even though the spec defines it as SEND-only (no CALLRESULT) - `Client`'s engine has no
  SEND-frame support yet; see the comment above its `ocpp_2_1_action!` invocation in
  `src/ocpp_2_1/actions.rs` for the tradeoff.
- **WebSocket is the only transport.** The engine (`Client<E>`) is transport-agnostic via the `Transport{Sink,Stream}`
  traits, but no second transport (e.g. an embedded framed link) exists yet.
- **Message types come from `ocpp-types`** (crates.io, `flowionab` org, currently `0.1.1`), not the
  earlier `rust-ocpp` fork - see `MIGRATION_OCPP_TYPES.md` for the full migration history, including a
  real codegen bug found and fixed upstream mid-migration (0.1.0 → 0.1.1: colliding inline-enum names in
  the 1.6 schema). `ocpp-types` is `no_std`, allocation-free by default (`heapless`-backed bounded types
  generated from the official JSON schemas per version), with an `alloc` feature - enabled unconditionally
  in this crate's `[dependencies]`, regardless of this crate's own `std` status - that turns
  spec-unbounded fields into plain `alloc::String`/`Vec` instead of const-generic-sized `heapless`
  collections. Bounded fields (e.g. `IdTag`) stay `heapless::String<N>` either way, which is the one
  API-shape difference from the old `rust-ocpp`-based types: construction goes through fallible
  `TryFrom`/`heapless::String::try_from(..)` rather than a bare string. `ocpp-types` has no per-version
  cargo features (`v16`/`v201`/`v21` always compile in) - this crate's own `ocpp_1_6`/`ocpp_2_0_1`/
  `ocpp_2_1` features just gate its own `src/ocpp_{1_6,2_0_1,2_1}/` modules, nothing to forward to.
  Its `serde` feature derives ordinary `serde::Serialize`/`Deserialize` (ordinary enough that this crate's
  existing `serde_json::Value`-based dynamic dispatch in `client.rs` needed no changes at all); its
  `serde-json-core` no-alloc JSON helpers on `Action` are unused here.
- **`no_std`+`alloc` works now.** `src/client.rs` no longer hard-depends on tokio or `async-trait`.
  `cargo build --lib --no-default-features --features ocpp_1_6` (or `ocpp_2_0_1`) compiles the engine with
  `std` off entirely - that's the standing proof this stays true; re-run it after touching `client.rs`,
  `transport.rs`, `error.rs`, or either per-version `error.rs`/`actions.rs`, all of which had std-only leaks
  (`std::error::Error`, `std::fmt`, `std::future::Future`, bare `String`/`format!`) fixed to their
  `core`/`alloc` equivalents as part of this work.
  - **Task spawning and timeouts are behind two small dyn-safe traits**, `Executor` and `Timer`
    (`src/runtime.rs`), the same "boxed trait object instead of a generic parameter" trick
    `TransportSink`/`TransportStream` already used - so `Client<E>` stays single-generic; only
    `Client::from_transport` takes `Box<dyn Executor>`/`Box<dyn Timer>` alongside the transport. Request/ping
    timeouts go through `runtime::with_timeout`, which races the caller's future against `Timer::delay` by
    hand (`core::future::poll_fn`) instead of depending on `futures::select`.
  - **`tokio-runtime` feature** (implied by `websocket`, which needs a tokio runtime for `tokio-tungstenite`
    regardless) provides `TokioExecutor`/`TokioTimer`, the impls `connect_1_6`/`connect_2_0_1` and this
    crate's own tests use. Embedded users disable it and supply their own `Executor`/`Timer` (e.g. backed by
    `embassy-executor`/`embassy-time`).
  - **Internal `Mutex`/oneshot/mpsc/broadcast replacements live in `src/sync.rs`**, built on
    `embassy-sync`'s `Mutex`/`Signal` fixed to `CriticalSectionRawMutex` (works under std via
    `critical-section`'s `std` backend, gated by our `std` feature; embedded targets must register their own
    backend via `critical_section::set_impl!`, standard embassy convention). The request-dispatch queue
    (`Chan`) is now unbounded (backed by `alloc::collections::VecDeque`) instead of the old fixed-capacity
    `mpsc::channel(1000)`, and the ping fan-out (`BroadcastRegistry`) gives each subscriber a single-slot
    `Signal` rather than tokio broadcast's buffered channel - a slow `on_ping` subscriber sees only the
    latest ping, not a backlog, which is fine for a low-frequency keepalive.
  - Diagnostics in `client.rs` go through the `tracing` crate (`tracing::warn!`/`error!`/`info!`), not a bare
    `eprintln!` - `tracing` itself is `#![no_std]` and used unconditionally (not gated behind our `std`
    feature), so these events fire the same way under `no_std`+`alloc` too. Our `std` feature forwards to
    `tracing/std` (thread-local dispatch instead of tracing's no_std global-only dispatch); either way, no
    events go anywhere unless the *application* installs a `tracing::Subscriber` (e.g. `tracing-subscriber`'s
    `fmt` layer on std, or a `defmt`/RTT-backed one on embedded) - this crate only emits events, it never
    installs a global subscriber itself. See `tests/ocpp_1_6_logging.rs` for how to capture them in a test
    (`tracing-test` with its `no-env-filter` feature - required because `tests/*.rs` files are each their own
    crate, so `tracing-test`'s default per-crate env filter would otherwise filter out `ocpp_client`'s own
    events).
  - True bare-metal no_std (no `alloc`) is still out of scope - the engine's `BTreeMap`/`VecDeque`/`Arc`-based
    bookkeeping is alloc-dependent by design. `ocpp-types` itself supports no_std+no_alloc (its `alloc`
    feature is opt-in, not required), but this crate always enables it - see the `ocpp-types` bullet above.

## Architecture

### One generic engine, not one implementation per version

`src/client.rs` defines `Client<E: ProtocolError>` — the entire CALL/RESULT/ERROR dispatch, timeout, and
pending-request bookkeeping, written **once** and shared by every OCPP version. `OCPP1_6Client` (in
`src/ocpp_1_6/mod.rs`) and `OCPP2_0_1Client` (in `src/ocpp_2_0_1/mod.rs`) are both just
`pub type ..Client = Client<..Error>;` — no duplicated client struct per version like the old design had.
Porting 2.0.1 required zero changes to `client.rs`/`error.rs`/`envelope.rs`/`transport.rs`, which is the
proof this design does what it was meant to.

- `src/error.rs` — `ProtocolError` trait (implemented once per version by that version's error enum: `code()`,
  `description()`, `details()`, `not_implemented()`, `from_wire()`) and `ClientError<E>`, the flattened error
  type (`Protocol(E)`, `Timeout`, `Decode(...)`, `Transport(...)`, `Closed`) replacing the old
  `Result<Result<Response, VersionError>, Box<dyn Error>>` double-nesting.
- `src/envelope.rs` — `RawCall`/`RawResult`/`RawError` tuple structs mirroring the OCPP-J wire arrays
  (`[MessageTypeId, UniqueId, Action, Payload]` etc.). Shared across all versions (the envelope shape is
  identical regardless of OCPP version) instead of duplicated per version.
- `src/action.rs` — the `Action` trait: `const NAME`, `type Request`, `type Response`. One marker type per
  OCPP action implements this (e.g. `ocpp_1_6::Heartbeat`). This is also the "custom message extension" path:
  implement `Action` for your own type (including one with vendor fields via `#[serde(flatten)]`) to send/
  receive it through the exact same `Client::call`/`Client::on` used for standard actions.
- `src/transport.rs` — `TransportSink`/`TransportStream` traits (dyn-safe via `async-trait`): send/receive one
  whole OCPP-J frame at a time, plus a `TransportEvent::{Frame,Ping,Pong}` so WebSocket-level keepalive can
  flow through without the generic engine knowing anything WebSocket-specific. `src/transport/websocket.rs`
  (feature `websocket`) is the only implementation today, wrapping a split `tokio-tungstenite` stream.
- `src/connect.rs` (feature `websocket`) — `connect_1_6()` does the WS handshake + protocol negotiation and
  builds a `Client` via `Client::from_transport(sink, stream, timeout)`. That constructor is `pub` precisely so
  tests (and eventually a non-WebSocket transport) can build a client over anything implementing the two
  transport traits, without needing a real socket — see `tests/common/mod.rs`'s in-memory fake transport.

### Per-version modules

`src/ocpp_1_6/` is the template for every version:
- `error.rs` — the version's `ProtocolError` impl. Written with one `macro_rules!` (`define_error!`) listing
  `Variant => "WireCode"` pairs once, instead of hand-writing `code()`/`description()`/`details()`/`from_wire()`
  match arms per variant.
- `actions.rs` — one `ocpp_1_6_action!(Name, Request, Response, "ActionName", send_x, on_x, wait_for_x)` macro
  invocation per OCPP action. Each expands to: an `Action` marker type, and `send_x`/`on_x`/`wait_for_x`
  methods on `OCPP1_6Client` that just call the generic `Client::call`/`Client::on`/`Client::wait_for`. This
  is the resolution of the earlier "ergonomic API" design discussion: callers get named, autocomplete-friendly
  methods (not `client.call::<Heartbeat>(...)` turbofish), but adding an action is one macro line, not three
  hand-written method bodies.
- `mod.rs` — re-exports plus the `OCPP1_6Client` type alias.

### Adding a new OCPP action to 1.6

Add one `ocpp_1_6_action!(...)` line in `src/ocpp_1_6/actions.rs` with the action's `ocpp_types::v16`
request/response types. Write the test first (a `tests/ocpp_1_6_fake_transport.rs`-style test is enough; no
need for a real WebSocket round-trip per action - one real-transport test already covers that wiring).

### Adding a new OCPP action to 2.0.1/2.1

Same as 1.6: add one `ocpp_2_0_1_action!(...)`/`ocpp_2_1_action!(...)` line in the matching `actions.rs`
with the action's `ocpp_types::v201`/`ocpp_types::v21` request/response types. Nested enum/field types
(e.g. `ResetRequestType`, or a version's `RpcErrorCode`) live under that version's `common` submodule, not
re-exported at the version root - only the top-level Request/Response structs are. Action name string = the
struct name minus its `Request`/`Response` suffix; `send_x`/`on_x`/`wait_for_x` method names are the
snake_case of that same name (the existing invocations in each file are the reference for edge cases like
acronym runs - `Get15118EVCertificate` → `get_15118_ev_certificate`, `ClearDERControl` →
`clear_der_control`). Write the test first, same TDD rule as every other version.

## TDD is mandatory

Every behavior added so far has a test in `tests/`. Two styles, both expected going forward, mirrored per
version (`ocpp_1_6_*` / `ocpp_2_0_1_*` / `ocpp_2_1_*`):

- **Fake-transport tests** (`tests/ocpp_1_6_fake_transport.rs`, `tests/ocpp_2_0_1_fake_transport.rs`,
  `tests/ocpp_2_1_fake_transport.rs`) — drive a real `Client` over the in-memory transport in
  `tests/common/mod.rs`. Fast, no networking, and the right default for anything about dispatch, timeouts, or
  error propagation.
- **Real-transport tests** (`tests/ocpp_1_6_websocket.rs`, `tests/ocpp_2_0_1_websocket.rs`,
  `tests/ocpp_2_1_websocket.rs`) — a real `tokio-tungstenite` server task plus
  `connect_1_6`/`connect_2_0_1`/`connect_2_1`, to prove the actual transport wiring works. One of these per
  version+transport combination is enough; don't duplicate every action-level test against a real socket.
- `wait_for_*` tests (`tests/ocpp_1_6_wait_for.rs`) need `#![cfg(feature = "test")]` at the top of the file and
  must be run with `cargo test --features test` — they're invisible to a plain `cargo test`.

## Common commands

This repo is a Cargo **workspace** (root `Cargo.toml` has `[workspace]` + `[package]` both -
`ocpp-client` is itself a member). `default-members = ["."]`, so a plain `cargo build`/`test`/etc.
at the root only ever touches `ocpp-client` - exactly like before the workspace existed. The two
satellite crates under `crates/` (`ocpp-transport-embassy-net`, `ocpp-board-stm32h723-nucleo` -
see "Embedded satellite crates" below) are never implicitly included; always pass `-p <crate>` to
touch them. (Passing `--workspace` bypasses `default-members` and builds everything, including
`ocpp-board-stm32h723-nucleo` - that fails on a non-ARM host, since it depends on `cortex-m`'s
inline `asm!`. Don't use `--workspace` here unless you also pass `--target thumbv7em-none-eabihf`.)

```sh
cargo build                              # default features: std, tokio-runtime, websocket, ocpp_1_6, ocpp_2_0_1, ocpp_2_1
cargo test                                # run all tests
cargo test <test_name>                    # run a single test by name (substring match)
cargo test --features test                # also run the wait_for_* tests
cargo build --no-default-features --features std,ocpp_1_6   # core + 1.6, no tokio/WebSocket pulled in
cargo build --lib --no-default-features --features ocpp_1_6 # no_std+alloc proof (no --target needed - lib-only, no binary to link)
cargo fmt                                 # format (rustfmt, per CONTRIBUTING.md)
```

CI (`.github/workflows/ci.yaml`) runs four jobs on push-to-`main` and every pull request: `fmt`
(`cargo fmt --all -- --check`), `clippy` (`cargo clippy --all-targets --all-features -- -D
warnings`), `test` (`cargo build` then `cargo test --features test`, covering the `wait_for_*`
tests too), `no_std` (the three `cargo build -p ocpp-client --lib --no-default-features --features
ocpp_{1_6,2_0_1,2_1}` proof builds), and `embedded` (checks/clippies
`ocpp-transport-embassy-net` and full-links `ocpp-board-stm32h723-nucleo` against the real
`thumbv7em-none-eabihf` target, with `RUSTFLAGS: --cfg getrandom_backend="custom"` - see that
crate's README for why). Keep all five green.

## Embedded satellite crates

`crates/ocpp-transport-embassy-net` (chip-agnostic `no_std`+`alloc` WebSocket transport over
`embassy-net`) and `crates/ocpp-board-stm32h723-nucleo` (NUCLEO-H723ZG firmware scaffold wiring
that transport to real Ethernet hardware) are the embedded story from PRODUCTION_READINESS.md
item 6. Both are real code (compile, and for the board crate, fully link, against
`thumbv7em-none-eabihf`) but **neither has been run against real hardware or a real CSMS yet** -
see each crate's own README for exact status, the two `ocpp-client`-relevant gaps their
existence surfaced (`getrandom` needs a custom backend on bare-metal targets; embassy's
`Spawner`/`Stack` are `!Sync` where `ocpp_client::Executor`/`Reconnector` require `Send + Sync`),
and next steps.
