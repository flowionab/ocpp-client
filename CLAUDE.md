# CLAUDE.md

Guidance for Claude Code (claude.ai/code) working in this repository.

## Project overview

`ocpp-client` implements the **charge point** (client) side of OCPP — the network/protocol layer
only, not charging logic or hardware control. Current version `0.5.0`; 0.4.0 is the newest on
crates.io. Pre-1.0 and still moving: 0.3.0 broke `TransportSink`/`TransportStream`,
`ReconnectPolicy` and `ConnectOptions`' defaults, 0.4.0 breaks every `dateTime` field's type
and adds a type parameter to the 2.x message types (see below), and 0.5.0 breaks only in that it
moves the public `ocpp-types` dependency from 0.2 to 0.3 (see below).

**OCPP 1.6, 2.0.1 and 2.1** are all implemented, tested end to end, and in `default` features. Every
action `ocpp-types` defines is wired up for every version (39 / 64 / 91) — `tests/action_coverage.rs`
fails the build otherwise. 1.6's 39 includes the eleven security-whitepaper actions, which arrived
with `ocpp-types` 0.2.0. WebSocket is the only production transport; an experimental `embassy-net`
one lives under `crates/` (see the bottom of this file).

Message types come from **`ocpp-types`** (crates.io, `flowionab` org, currently `0.3.0`) — see
`MIGRATION_OCPP_TYPES.md` for the migration off the old `rust-ocpp` fork and for what each bump
changed. 0.3.0 was purely additive (one opt-in `validate` feature, not enabled here — no type,
field or action changed shape, and nothing in `src/` needed editing); 0.2.0 is the release that
reshaped everything below. It's `no_std`, with `alloc` on by default since 0.2.0; this crate enables `alloc`
unconditionally either way, so spec-unbounded fields are plain `String`/`Vec`. Bounded fields (e.g.
`IdTag`) stay `heapless::String<N>`, so construction goes through fallible `TryFrom`. Two shapes
worth knowing before touching a per-version file:

- **`dateTime` fields are `ocpp_types::OcppTimestamp`**, not strings — 16 bytes, comparable, built
  with `OcppTimestamp::parse_rfc3339`. The client parses them now, so a malformed one from the
  peer is a `ClientError::Decode`, and equality is by instant, not by written offset.
  `tests/ocpp_1_6_timestamps.rs` pins that behaviour.
- **2.0.1/2.1 message types take a `customData` type parameter.** This crate's action markers
  carry it too (`Reset<AcmeExtension>`), defaulting to the spec's `CustomData`; see
  `ocpp_2_0_1_action!`'s doc comment for why the generated methods stay concrete.

`ocpp-types` has no per-version cargo features; this crate's `ocpp_1_6`/`ocpp_2_0_1`/`ocpp_2_1`
features only gate its own modules. Two optional features forward to `ocpp-types`' own: `chrono`
adds `OcppTimestamp` ↔ `chrono::DateTime` conversions and nothing else, and `validate` adds the
`Validate` trait (the spec bounds the types can't carry — `maxLength` on fields too large to
inline, plus every `minItems`/`minimum`/`maximum`/`multipleOf`) **plus** this crate's
`From<ValidationError>` for each version's error type.

**Nothing validates automatically, and `Client::call` must not start.** Doing so needs a
`Validate` bound on `Action::Request`; behind a `#[cfg]` that makes the `Action` trait change
shape with a feature flag, and since cargo features unify across the graph, one crate enabling
`validate` would break another's custom `Action` impl — the extension path `src/action.rs`
advertises. It would also take `ClientError` from 64 to ~304 bytes (`ValidationError` is 296,
path held inline). Callers write `request.validate()?`. Full reasoning in
PRODUCTION_READINESS.md item 5; don't relitigate it without reading that first.

## Architecture

### One generic engine, not one implementation per version

`src/client.rs` defines `Client<E: ProtocolError>` — the entire CALL/RESULT/ERROR dispatch, timeout
and bookkeeping, written **once**. `OCPP1_6Client`/`OCPP2_0_1Client`/`OCPP2_1Client` are just
`pub type ..Client = Client<..Error>;`. Porting 2.0.1 and 2.1 required zero changes to
`client.rs`/`error.rs`/`envelope.rs`/`transport.rs`, which is the proof this design does its job.

- `src/error.rs` — the `ProtocolError` trait (implemented once per version) and `ClientError<E>`
  (`Protocol`/`Timeout`/`Decode`/`Transport`/`Closed`).
- `src/envelope.rs` — `RawCall`/`RawResult`/`RawError`/`RawSend` tuple structs mirroring the OCPP-J
  wire arrays. Shared by all versions; the envelope shape doesn't differ between them.
- `src/action.rs` — the `Action` trait (`const NAME`, `type Request`, `type Response`) and its
  single-payload sibling `SendAction` for OCPP-J 2.1 `SEND` (message type 6, fire-and-forget). This
  is also the custom-message extension path: implement `Action` for your own type (vendor fields via
  `#[serde(flatten)]`) and it goes through the same `Client::call`/`Client::on`.
- `src/transport.rs` — `TransportSink`/`TransportStream`, dyn-safe via hand-written boxed futures
  (**not** the `async-trait` crate, which is deliberately not a dependency), plus
  `TransportEvent::{Frame, Ping(Vec<u8>), Pong(Vec<u8>)}`. `src/transport/websocket.rs` (feature
  `websocket`) wraps a split `tokio-tungstenite` stream.
- `src/keepalive.rs` — `KeepalivePolicy`/`KeepaliveBehavior`, deliberately shaped like
  `src/reconnect.rs`'s policy/behavior pair. The loop itself is `keepalive_loop` in `client.rs`.
- `src/runtime.rs` — `Executor`/`Timer` traits plus `with_timeout`/`with_cancel`, both hand-rolled
  `poll_fn` races (no `futures::select` dependency). `runtime/tokio.rs` has the std impls.
- `src/sync.rs` — `SharedMutex`/`OneShot`/`Chan`/`Notify`/`BroadcastRegistry`, replacing the tokio
  primitives `client.rs` used to need. Built on `embassy-sync` fixed to `CriticalSectionRawMutex`.
- `src/connect.rs` (feature `websocket`) — `connect_1_6`/`_2_0_1`/`_2_1`/`connect` do the handshake
  and protocol negotiation, then build a `Client` via
  `Client::from_transport_with_config(sink, stream, executor, timer, config)`. Options live in
  `ClientConfig`, not a positional list; the older `from_transport`/`from_transport_with_reconnect`
  remain as thin wrappers. `ConnectOptions::default()` enables reconnect **and** keepalive, while
  `ClientConfig::new` enables neither — the convenience path opts in, the raw-transport path stays
  inert.

### Invariants that are easy to break

These each cost a real bug once. Read them before touching `client.rs`.

- **Every bookkeeping entry must be removed by whoever added it, on every exit path.** `Client` keeps
  four tables (`pending_responses`, `pong_waiters`, `request_senders`, `notification_senders`). The
  read loop only removes an entry when a *response arrives*, so any path that gives up — timeout,
  transport send failure — must clean up itself or the table grows forever. `do_send_request` and
  `send_ping_with_timeout` have each shipped this bug once. `tests/ocpp_1_6_bookkeeping.rs` guards
  both via the `test`-gated `pending_request_count`/`pending_ping_count` accessors; add a case there
  when you add a table.
- **Retiring a handler means closing its channel, not just dropping the map entry.** `on`/
  `on_notification` spawn a task parked on a `Chan`; overwriting the registration alone leaves that
  task alive forever. `Chan::close` ends it, and drains first so the outgoing handler still answers
  calls already dispatched to it. `wait_for` likewise removes its own registration on return, guarded
  by `Chan::is_same`.
- **Pongs are matched by correlation token, not arrival order.** Pings carry an 8-byte token as
  payload and `PongState` keys outstanding pings by it. Positional matching meant one timed-out or
  unsolicited pong desynced every later ping permanently. Transports must therefore carry ping/pong
  payloads verbatim (RFC 6455 requires a pong to echo the ping's).
- **The read loop has three distinct exit paths** — `LoopExit::{Eof, Forced, Shutdown}` — and
  collapsing them is a bug. `Eof` redials if configured; `Forced` (keepalive gave up, or
  `force_reconnect`) redials after a bounded courtesy close; `Shutdown` (`disconnect()`, via a sticky
  `closed: AtomicBool`) exits without redialling. `closed` also short-circuits the keepalive loop,
  `set_ping_interval`, `force_reconnect` and every send path (which return `ClientError::Closed`).
  Anything new reacting to the connection ending must check it.
- **`recv` is raced against the wake signal unconditionally**, so `disconnect()` can pull the loop
  out of a parked `recv`. That's why `TransportStream::recv`'s cancel-safety contract is
  unconditional too.
- **Reconnect backoff escalates on "this connection never worked", not "the dial failed".**
  `attempt` lives across connections and resets *only* when inbound traffic arrives — never on a dial
  merely completing, which is what made an accept-then-immediately-close peer a zero-delay hot loop
  (~5k dials/s, measured). The delay applies *before* every dial for the same reason.
  `ReconnectPolicy::jitter` (default on) spreads delays over `[d/2, d]` so a fleet doesn't retry in
  lockstep. Keep both halves: escalation must not punish a connection that genuinely worked.
- **`no_std` + `alloc` must keep compiling.** `client.rs`, `transport.rs`, `error.rs`, `runtime.rs`,
  `sync.rs` and the per-version files have all leaked `std` before (`std::error::Error`, `std::fmt`,
  bare `String`/`format!`). Re-run the three proof builds after touching any of them. Diagnostics go
  through `tracing` (itself `#![no_std]`, used unconditionally); this crate never installs a
  subscriber. `tests/ocpp_1_6_logging.rs` shows how to capture events in a test — it needs
  `tracing-test`'s `no-env-filter` feature, since each `tests/*.rs` is its own crate.
- True bare-metal `no_std` (no `alloc`) is out of scope: the engine's `BTreeMap`/`VecDeque`/`Arc`
  bookkeeping is alloc-dependent by design. See PRODUCTION_READINESS.md item 9.

### Per-version modules

`src/ocpp_1_6/` is the template:
- `error.rs` — that version's error struct and `ProtocolError` impl: a `wire_code` match mapping
  `RpcErrorCode` to its exact wire spelling, and `from_wire` matching back. (There is no
  `define_error!` macro; this file claimed one for a while and was wrong.) Under `validate`, also
  that version's `From<ValidationError>`, which is the one place the versions genuinely differ —
  1.6 spells it `OccurenceConstraintViolation`, 2.x `Occurrence`. Shared rendering for the
  description and `errorDetails` lives once in `src/error.rs::validation_error_parts`.
- `actions.rs` — one `ocpp_1_6_action!(Name, Request, Response, "ActionName", send_x, on_x,
  wait_for_x)` per action, expanding to the marker type plus the three methods. Adding an action is
  one macro line, but callers still get named, autocomplete-friendly methods.
- `mod.rs` — re-exports plus the `OCPP1_6Client` alias.

### Adding an action

Add one `ocpp_{1_6,2_0_1,2_1}_action!(...)` line in that version's `actions.rs` with the
`ocpp_types::{v16,v201,v21}` request/response types. **Write the test first.** The 2.x macros take
the request/response as `ident`s, not `ty`s, because they append the `customData` parameter to
them — pass the bare type name and nothing else. Nested enum/field
types live under that version's `common` submodule, not the version root. The action name string is
the struct name minus its `Request`/`Response` suffix; method names are its snake_case (existing
invocations are the reference for acronym runs — `Get15118EVCertificate` →
`get_15118_ev_certificate`, `ClearDERControl` → `clear_der_control`). 2.1's SEND-only
`NotifyPeriodicEventStream` uses `ocpp_2_1_send_action!` instead, since it has no response type.

## TDD is mandatory

Every behavior has a test in `tests/`. Two styles, mirrored per version:

- **Fake-transport** (`tests/ocpp_{1_6,2_0_1,2_1}_fake_transport.rs`) — drive a real `Client` over
  the in-memory transport in `tests/common/mod.rs`. The right default for dispatch, timeouts and
  error propagation.
- **Real-transport** (`tests/ocpp_{1_6,2_0_1,2_1}_websocket.rs`) — a real `tokio-tungstenite` server
  plus `connect_*`. One per version+transport is enough; don't duplicate action-level tests over a
  real socket.

Tests needing the `test` feature (`wait_for_*`, bookkeeping) are invisible to a plain `cargo test`;
run `cargo test --features test`.

Some suites don't fit either style and aren't mirrored per version:
- `tests/action_coverage.rs` — locates the `ocpp-types` source via `cargo metadata`, reads the
  `const ACTION` from each `*_request.rs`, and fails if any action lacks a macro invocation. Guards
  the gap that shipped in 0.2.0 (five actions with types present but no wrapper), and is what
  caught the eleven 1.6 security actions arriving in `ocpp-types` 0.2.0. If `ocpp-types` changes
  its one-file-per-type layout, this is what breaks.
- `tests/ocpp_1_6_keepalive.rs` / `_websocket_keepalive.rs` — ping/pong and scheduled keepalive.
  Version-independent, so 1.6 only. Millisecond intervals with generous `timeout(..)` bounds rather
  than a mock clock; the real-socket file exists to prove a real peer echoes ping payloads.
- `tests/ocpp_1_6_bookkeeping.rs` / `_disconnect.rs` / `_reconnect_backoff.rs` — the invariants
  above. Each case was confirmed to fail against the unfixed code; keep that property.
- `tests/ocpp_1_6_timestamps.rs` — what a peer may write in a `dateTime` field and what happens
  when it writes nonsense. Version-independent (the type is shared), so 1.6 only. The `chrono`
  case is `#[cfg(feature = "chrono")]`, so it needs `cargo test --features test,chrono`.
- `tests/validation_error_mapping.rs` — `From<ValidationError>` for all three error types. The
  whole file is `#![cfg(feature = "validate")]`, so without `--features validate` it compiles to
  nothing and reports zero tests rather than failing. Not mirrored per version: the point is the
  three versions differing. Note 1.6's occurrence case is built by hand — no 1.6 schema states
  `minItems`, so no generated 1.6 message can produce that class.
- `tests/custom_data_generics.rs` — the 2.x `customData` type parameter, through both `call` and
  `on`, including that the marker's default keeps the spec shape rather than `NoCustomData`.
- `tests/ocpp_1_6_security_actions.rs` — one round trip per security-whitepaper action, split by
  who initiates it. `action_coverage` proves the wiring exists; this proves it works.

## Common commands

This repo is a Cargo **workspace** whose root is also the `ocpp-client` package.
`default-members = ["."]`, so a plain `cargo build`/`test` only touches `ocpp-client`. The two
satellite crates under `crates/` need an explicit `-p`. Don't use `--workspace` without
`--target thumbv7em-none-eabihf` — `ocpp-board-stm32h723-nucleo` depends on `cortex-m`'s inline
`asm!` and won't build for the host.

```sh
cargo build                                   # default: std, tokio-runtime, websocket, all three versions
cargo test --features test                    # all tests, including the test-gated ones
cargo test --features test,chrono,validate    # what CI runs: adds the chrono-interop case in
                                              # tests/ocpp_1_6_timestamps.rs and all of
                                              # tests/validation_error_mapping.rs
cargo test <name>                             # single test, substring match
cargo fmt                                     # rustfmt, per CONTRIBUTING.md
cargo build --no-default-features --features std,ocpp_1_6      # core + 1.6, no tokio/WebSocket
cargo build --lib --no-default-features --features ocpp_1_6    # no_std+alloc proof (lib-only, no --target needed)
```

MSRV is **1.87** for the library (`ocpp-types` declares the same, and is what binds it); the test
suite needs 1.88 for dev-dependencies. Verify with `cargo +1.87 check --lib --all-features`.

CI (`.github/workflows/ci.yaml`) runs five jobs on push-to-`main` and every PR: `fmt`, `clippy`
(`--all-targets --all-features -D warnings`), `test` (`cargo build` then
`cargo test --features test,chrono,validate`), `no_std` (the three lib-only proof builds), and `embedded`
(checks/clippies `ocpp-transport-embassy-net` and full-links `ocpp-board-stm32h723-nucleo` against
the real `thumbv7em-none-eabihf` target, with `RUSTFLAGS: --cfg getrandom_backend="custom"`). Keep
all five green — the `embedded` job in particular catches dead code that host builds don't.

## Embedded satellite crates

`crates/ocpp-transport-embassy-net` (chip-agnostic `no_std`+`alloc` WebSocket transport over
`embassy-net`) and `crates/ocpp-board-stm32h723-nucleo` (NUCLEO-H723ZG firmware scaffold) are
PRODUCTION_READINESS.md item 6. Both compile — and the board crate fully links — against
`thumbv7em-none-eabihf`, but **neither has been run against real hardware or a real CSMS**, and the
transport has no TLS. Each crate's README states its exact status and the two `ocpp-client`-relevant
gaps their existence surfaced: `getrandom` needs a custom backend on bare-metal targets, and
embassy's `Spawner`/`Stack` are `!Sync` where `Executor`/`Reconnector` require `Send + Sync`.
