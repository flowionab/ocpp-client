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

2. ~~**No session-resilience layer on top of reconnection.**~~ **Partially done.**
   `Client::on_reconnect` (mirrors `on_ping`) fires a callback every time the background read
   loop redials successfully - never on the initial connection, only on later reconnects, so
   it's the natural place for a caller to re-run `BootNotification` or resync other session
   state. Actually re-running `BootNotification`/replaying `StatusNotification` automatically
   is still not this crate's job - it's a "protocol layer only" crate (that's what
   `ocpp-charge-point` is for) - but the hook to build that on top of now exists, where before
   there was no reconnect signal at all to hook into. Covered by
   `tests/ocpp_1_6_on_reconnect.rs` (fake-transport test with a custom `Reconnector`, proving
   the callback fires after a redial and not before).

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
   reporting until upstream lands it or the fork is patched. Not otherwise actionable from
   here (nothing to fix in this crate until upstream lands it), so instead of a code change,
   flagged it in the README's supported-protocols table so users don't discover the gap by
   surprise.

5. **Versioning/release.** Crate is `0.2.0-alpha.1` and depends on a git fork of `rust-ocpp`
   rather than a crates.io release. Not publishable to crates.io as-is (git deps generally
   block `cargo publish` for a lib unless the fork is also published); API is alpha-stability.

6. **README overclaims embedded transport support.** It advertises "STM32-compatible
   transport layer" / "STM32 transport: ✅ Supported," but only the WebSocket transport
   actually exists. `no_std`+`alloc` compiles and the `TransportSink`/`TransportStream` traits
   exist to build one against, but no embedded transport implementation ships today. **In
   progress.** `crates/ocpp-transport-embassy-net` (new workspace member) is a chip-agnostic
   `no_std`+`alloc` WebSocket transport over `embassy-net`, built on `embedded-websocket`'s
   sans-io core - `cargo check`/`clippy -D warnings` pass against the real
   `thumbv7em-none-eabihf` target (see that crate's README for the exact commands), but it has
   not been run against real hardware or a real CSMS yet, has no TLS support, and simplifies the
   WebSocket close handshake (no close-reply frame sent). Checking it against a real embedded
   target (not just the host, which the top-level `no_std` CI job only does) surfaced two things
   worth knowing about `ocpp-client` itself, documented in the new crate's README:
   - `uuid`'s `v4` feature (used for OCPP-J message IDs) pulls in `getrandom`, which has no
     backend on bare-metal `thumbv7em-none-eabihf` without the firmware binary opting into
     getrandom's "custom backend" mechanism (`--cfg getrandom_backend="custom"` plus a
     hardware-RNG-backed extern fn) - invisible from `ocpp-client`'s own no_std CI job, which
     builds for the host, where a real OS-provided entropy source always exists.
   - `ocpp_client::Executor`/`Reconnector` are declared `Send + Sync + 'static` (right for
     tokio's multi-threaded model, which is the only `Executor` impl that exists today) but
     `embassy_executor::Spawner`/`embassy_net::Stack` are deliberately `!Sync` (`Spawner` is
     `!Send` too) since embassy's single-core cooperative executor has no real concurrent access
     to guard against. The new crate bridges this with a handful of documented `unsafe impl
     Send`/`Sync` assertions rather than a design change in `ocpp-client` itself; still worth
     being aware this trait boundary was written with only a multi-threaded runtime in mind.
   Still needed: an STM32H723-specific board-support crate (Ethernet MAC/PHY bring-up via
   `embassy-stm32`) wiring into this transport, then hardware-in-the-loop testing, then TLS -
   see that crate's README's "Next steps".

7. ~~**No structured logging/telemetry.**~~ **Done.** Every diagnostic in `client.rs` (malformed
   frames, unparsable CALL/CALLRESULT/CALLERROR, failed response encode/send, reconnect
   attempts and successes) now goes through the `tracing` crate (`warn!`/`error!`/`info!` with
   structured fields, e.g. `error = %err`, `attempt`) instead of a bare `std`-gated `eprintln!`.
   `tracing` is `#![no_std]` itself and used unconditionally - not gated behind our `std`
   feature - so these events fire under `no_std`+`alloc` too; our `std` feature now forwards to
   `tracing/std` for thread-local dispatch. This crate never installs a global `Subscriber`
   itself (that's the application's job - `tracing-subscriber`'s `fmt` layer on std, a
   `defmt`/RTT-backed one on embedded), so there's zero behavior change for anyone not already
   listening. Covered by `tests/ocpp_1_6_logging.rs` (`tracing-test`, dev-only, with its
   `no-env-filter` feature - required because `tests/*.rs` integration tests are each their own
   crate, so the default per-crate env filter would otherwise hide `ocpp_client`'s events).

8. ~~**TLS trust config isn't exposed.**~~ **Done.** `ConnectOptions::tls_config: Option<Arc<rustls::ClientConfig>>`
   lets callers supply their own `rustls::ClientConfig` (custom root CA, mTLS client certs,
   whatever `rustls` supports) instead of the default public-CA-only `webpki-roots` trust
   store; `None` keeps the old default behavior. Threaded through reconnect too - the
   `WebSocketReconnector` reuses the same config on every redial. `ocpp_client::rustls` now
   re-exports the exact `rustls` version this crate was built against, so callers don't have to
   pin a matching version themselves. Covered by `tests/ocpp_1_6_custom_tls.rs`: one test proves
   a `wss://` connection succeeds against a self-signed cert when its exact cert is the
   configured root, another proves the *default* config still rejects that same self-signed
   cert (i.e. the escape hatch doesn't weaken the default trust posture). `rcgen` (dev-only,
   `aws_lc_rs` backend to match `rustls`'s default and keep only one crypto provider in the
   graph) generates the test certificate.
