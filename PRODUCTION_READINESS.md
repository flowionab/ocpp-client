# Production readiness

Gaps to close before pointing real hardware/fleets at this crate, in priority order.

1. ~~**No reconnection logic.**~~ **Done.** `Client<E>`'s background read loop now takes an
   optional `Reconnector` (`src/reconnect.rs`) plus a `ReconnectPolicy` (bounded exponential
   backoff, unlimited attempts) via `Client::from_transport_with_reconnect`. `connect_1_6` /
   `connect_2_0_1` / `connect_2_1` wire up a WebSocket-backed `Reconnector` automatically and
   reconnect **on by default** (`ConnectOptions::reconnect: ReconnectBehavior`, default
   `Enabled(ReconnectPolicy::default())`; set to `Disabled` to opt out). Embedded/non-WebSocket
   transports get the same behavior for free by implementing `Reconnector` themselves. Covered
   by `tests/ocpp_1_6_reconnect.rs` (real socket drop + redial) and a unit test for the backoff
   math. Not yet covered: failing in-flight requests immediately on disconnect (they still wait
   out the full timeout) and a hook for re-running `BootNotification`/state resync after
   reconnect — see item 2.

2. **No session-resilience layer on top of reconnection** — e.g. re-running
   `BootNotification` on reconnect, replaying `StatusNotification`. Arguably out of scope for
   a "protocol layer only" crate (that's what `ocpp-charge-point` is for), but worth being
   explicit that integrators must build it themselves.

3. ~~**CI is thin.**~~ **Done.** `.github/workflows/ci.yaml` now runs four parallel jobs on
   push-to-`main` and on every pull request: `fmt` (`cargo fmt --all -- --check`), `clippy`
   (`cargo clippy --all-targets --all-features -- -D warnings`), `test` (`cargo build` then
   `cargo test --features test`, so the `wait_for_*` tests are no longer invisible to CI), and
   `no_std` (the `cargo build --lib --no-default-features --features ocpp_{1_6,2_0_1,2_1}`
   proof build for each version, per CLAUDE.md's embedded-support guarantee). Fixed the one
   pre-existing clippy warning that blocked `-D warnings` (a collapsible `if` in
   `connect.rs::setup_socket`) and silenced `result_large_err` in the WebSocket test files
   (`tokio-tungstenite`'s handshake closure signature makes that one unavoidable without
   restructuring the test helper).

4. **`NotifyReport` (OCPP 2.1) is unimplemented** — blocked upstream on `rust-ocpp`'s
   `wip_v2_1::messages::notify_report` being an empty module. Blocks 2.1 device-model
   reporting until upstream lands it or the fork is patched.

5. **Versioning/release.** Crate is `0.2.0-alpha.1` and depends on a git fork of `rust-ocpp`
   rather than a crates.io release. Not publishable to crates.io as-is (git deps generally
   block `cargo publish` for a lib unless the fork is also published); API is alpha-stability.

6. **README overclaims embedded transport support.** It advertises "STM32-compatible
   transport layer" / "STM32 transport: ✅ Supported," but only the WebSocket transport
   actually exists. `no_std`+`alloc` compiles and the `TransportSink`/`TransportStream` traits
   exist to build one against, but no embedded transport implementation ships today. Fix the
   claim or ship the transport.

7. **No structured logging/telemetry.** Diagnostics go through `eprintln!` gated on `std`,
   no `log`/`defmt`/`tracing` facade. Fine for a hobby project, awkward for production ops
   (no log levels, nothing embedded targets can hook into).

8. **TLS trust config isn't exposed.** `connect.rs` uses `rustls-tls-webpki-roots` (public CA
   validation only). No way to supply a custom root store through this crate's `connect_*`
   functions, which blocks CSMS backends that use a private/internal CA.
