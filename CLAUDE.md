# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

`ocpp-client` is a Rust crate implementing the **charge point** (client) side of OCPP — the network/protocol
layer only, not charging logic or hardware control. It's mid-rewrite (`1.0.0-alpha.1`) from a previous
per-version, tokio-hardwired design (see git history before this version bump) to a single generic engine
shared by every OCPP version, with a transport abstraction so WebSocket isn't the only option long-term.

## Status: 1.6 + 2.0.1 + 2.1, WebSocket only, no_std+alloc capable

- **OCPP 1.6, 2.0.1, and 2.1** are all implemented and tested end to end, all three in `default` features.
  2.1 mirrors `src/ocpp_2_0_1/` exactly (`src/ocpp_2_1/{mod,error,actions}.rs`, `OCPP2_1Client`,
  `connect_2_1()`) - `OCPP2_1Error` reuses the same RPC framework error codes as `OCPP2_0_1Error` (OCPP-J's
  envelope/error-code set didn't change between 2.0.1 and 2.1). One action is missing:
  **`NotifyReport` has no `ocpp_2_1_action!` entry** because `rust-ocpp`'s
  `wip_v2_1::messages::notify_report` module is still an empty file upstream (confirmed by reading the
  fork's checked-out source, not just its feature flag - the same trap the crate's `Cargo.toml` used to warn
  about before this was ported). Add the `NotifyReportRequest`/`NotifyReportResponse` macro invocation once
  that lands upstream; every other 2.1 action (85 total minus `NotifyReport`) is wired up. `rust-ocpp`'s
  `Cargo.toml` still calls `wip_v2_1` "not quite ready for use yet," but in practice only that one action is
  actually blocked - re-verify against the fork's current `src/v2_1/messages/` if bumping the dependency.
- **WebSocket is the only transport.** The engine (`Client<E>`) is transport-agnostic via the `Transport{Sink,Stream}`
  traits, but no second transport (e.g. an embedded framed link) exists yet.
- **The `rust-ocpp` dependency is a fork**, not the crates.io release: `rust-ocpp = { git =
  "https://github.com/flowionab/rust-ocpp" }` in `Cargo.toml`. The upstream crate on crates.io is NOT no_std
  viable - confirmed via `cargo tree`, it unconditionally pulls in `jsonschema`/`reqwest`/`hyper`/`tokio` plus
  `validator` (used via `#[derive(Validate)]` across ~243 files) which itself unconditionally requires
  `url`/`idna`/`regex(std)`. The fork removes all of that: `jsonschema`/`reqwest`/`hyper` are gone entirely,
  and `validator`/`regex` are optional, gated behind the fork's own `std` feature
  (`#[cfg_attr(feature = "std", derive(validator::Validate))]` on every message struct) - confirmed via
  `cargo tree --no-default-features --features ocpp_1_6`, which no longer shows any of them. It also has a
  real, working `wip_v2_1` feature (unlike the crates.io release, where `v2_1`'s Cargo feature is commented
  out even though the module source exists). Our own `std` Cargo feature now forwards to `rust-ocpp/std`.
  If bumping this dependency, re-verify with `cargo tree` that none of the removed deps have crept back in.
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
  - Diagnostic `eprintln!` calls in `client.rs` are gated `#[cfg(feature = "std")]` (silently dropped
    otherwise) - no `log`/`defmt` facade wired up yet; that's a reasonable follow-up if embedded users need
    that output surfaced.
  - True bare-metal no_std (no `alloc`) is still out of scope - the engine's `BTreeMap`/`VecDeque`/`Arc`-based
    bookkeeping is alloc-dependent by design, matching `rust-ocpp`'s own no_std+alloc (not no_std+no_alloc)
    support.

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

Add one `ocpp_1_6_action!(...)` line in `src/ocpp_1_6/actions.rs` with the action's `rust_ocpp` request/response
types. Write the test first (a `tests/ocpp_1_6_fake_transport.rs`-style test is enough; no need for a real
WebSocket round-trip per action - one real-transport test already covers that wiring).

### Adding a new OCPP action to 2.1 (e.g. once `NotifyReport` lands upstream)

Same as 1.6: add one `ocpp_2_1_action!(...)` line in `src/ocpp_2_1/actions.rs` with the action's `rust_ocpp`
request/response types from `rust_ocpp::v2_1::messages::<module>`. Action name string = the struct name
minus its `Request`/`Response` suffix; `send_x`/`on_x`/`wait_for_x` method names are the snake_case of that
same name (the existing invocations in that file are the reference for edge cases like acronym runs -
`Get15118EVCertificate` → `get_15118_ev_certificate`, `ClearDERControl` → `clear_der_control`). Write the
test first, same TDD rule as every other version.

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

```sh
cargo build                              # default features: std, tokio-runtime, websocket, ocpp_1_6, ocpp_2_0_1, ocpp_2_1
cargo test                                # run all tests
cargo test <test_name>                    # run a single test by name (substring match)
cargo test --features test                # also run the wait_for_* tests
cargo build --no-default-features --features std,ocpp_1_6   # core + 1.6, no tokio/WebSocket pulled in
cargo build --lib --no-default-features --features ocpp_1_6 # no_std+alloc proof (no --target needed - lib-only, no binary to link)
cargo fmt                                 # format (rustfmt, per CONTRIBUTING.md)
```

CI (`.github/workflows/ci.yaml`) runs `cargo build` and `cargo test` (default features) on push — keep both
green. It does not currently run with `--features test`; if you add a `wait_for_*`-only test, it won't be
caught by CI unless you also run it locally.
