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

4. ~~**`NotifyReport` (OCPP 2.1) is unimplemented**~~ **Done.** Resolved by migrating message
   types from the `rust-ocpp` fork to [`ocpp-types`](https://github.com/flowionab/ocpp-types)
   (see `MIGRATION_OCPP_TYPES.md`), which ships a real `NotifyReportRequest`/`Response`.
   `ocpp_2_1_action!(NotifyReport, ...)` is wired up in `src/ocpp_2_1/actions.rs` like every
   other action, covered by a fake-transport test proving a full CALL/CALLRESULT round trip
   (`tests/ocpp_2_1_fake_transport.rs::call_resolves_notify_report_now_that_its_wired_up`).
   Every 2.1 action is now implemented, including `NotifyPeriodicEventStream` as a genuine
   `SEND` (OCPP-J message type 6, fire-and-forget, no CALLRESULT) rather than the earlier
   call/response modeling workaround - `Client`'s engine gained real SEND-frame support
   (`Client::send_notification`/`Client::on_notification`, backed by the new `SendAction`
   trait) and `src/ocpp_2_1/actions.rs` wires the action up via the new
   `ocpp_2_1_send_action!` macro. Covered by
   `tests/ocpp_2_1_fake_transport.rs::send_notify_periodic_event_stream_writes_a_send_frame_and_does_not_wait_for_a_reply`
   and `::on_notify_periodic_event_stream_fires_and_never_sends_a_reply`, the latter also
   asserting the client never auto-replies to a received SEND.

5. **Versioning/release.** Crate is `0.2.0-alpha.1`. The git-fork dependency blocker is gone -
   `ocpp-types` is a real crates.io release (`0.1.1`), not a git dependency, so that specific
   `cargo publish` obstacle no longer applies. `ocpp-types` itself is early (`0.1.x`, same org as
   the old `rust-ocpp` fork - see `MIGRATION_OCPP_TYPES.md`'s Risk section for a codegen bug
   found and fixed upstream mid-migration), so pin it deliberately rather than assuming API
   stability. API here is still alpha-stability regardless.

6. ~~**README overclaims embedded transport support.**~~ **Done.** It advertised "STM32-compatible
   transport layer" / "STM32 transport: ✅ Supported," but only the WebSocket transport
   actually existed. `no_std`+`alloc` compiles and the `TransportSink`/`TransportStream` traits
   exist to build one against, and now a real embedded transport implementation ships too.
   `crates/ocpp-transport-embassy-net` (new workspace member) is a chip-agnostic
   `no_std`+`alloc` WebSocket transport over `embassy-net`, built on `embedded-websocket`'s
   sans-io core - `cargo check`/`clippy -D warnings` pass against the real
   `thumbv7em-none-eabihf` target (see that crate's README for the exact commands). It has not
   been run against real hardware or a real CSMS, has no TLS support, and simplifies the
   WebSocket close handshake (no close-reply frame sent) - accepted as-is: any real board deploy
   requires hardware-specific customization anyway, so hardware-in-the-loop validation and TLS
   are left to downstream integrators rather than blocking this crate's readiness. Checking it
   against a real embedded
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
   `crates/ocpp-board-stm32h723-nucleo` (new workspace member, excluded from
   `default-members` since it needs a real ARM target) is the board-support crate: clock tree
   and NUCLEO-H723ZG's onboard LAN8742A Ethernet RMII pin mapping (cross-checked against two
   independent working reference sources - see that crate's README), hardware RNG shared
   between `ocpp-transport-embassy-net`'s `RngFactory` and `getrandom`'s custom backend (closing
   the loop on the `getrandom` gap above - `cargo build` there does a *full link*, not just
   `check`, specifically to catch the missing-custom-backend-symbol failure that a `check`-only
   CI job wouldn't). Builds and links for `thumbv7em-none-eabihf`; not yet flashed to real
   hardware or tested against a real CSMS, which is expected to happen per-board during
   downstream integration rather than in this repo. CI's new `embedded` job covers both new
   crates against the real target on every push/PR (`.github/workflows/ci.yaml`). See
   `ocpp-transport-embassy-net`'s README's "Next steps" for hardware-in-the-loop testing and TLS
   as optional follow-ups for integrators.

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
   graph) generates the test certificate. OCPP Security Profile 3 (mutual TLS: client presents a
   certificate, no HTTP Basic Auth) is covered too - `tests/ocpp_1_6_mtls.rs` builds a
   `ClientConfig` via `.with_client_auth_cert(..)` and a server `ServerConfig` with a
   `WebPkiClientVerifier` that only trusts that one client cert, proving a full CALL/CALLRESULT
   round trip over mTLS; a second test proves the server rejects a client presenting no
   certificate at all. Profiles 1/2/3 aren't modeled as a named concept anywhere in the crate -
   they fall out of whatever the caller puts in `ConnectOptions` (`tls_config` +
   `username`/`password`).

9. **True bare-metal no_std, no `alloc`.** `Client<E>`'s bookkeeping (`src/client.rs`) is
   `alloc`-dependent by design: `Arc` for cross-task shared ownership, `BTreeMap` for the
   pending-request/handler tables, `VecDeque` for pong waiters, `Box<dyn TransportSink/
   TransportStream/Executor/Timer>` for the dyn-trait transport/runtime abstraction, plus
   `String`/`format!` for error messages. `ocpp-types` itself supports no-alloc (bounded
   `heapless` types), but this crate enables its `alloc` feature unconditionally regardless of
   this crate's own `std` status. Closing this gap means replacing the above with fixed-capacity
   `heapless` equivalents sized by const generics (bounding max in-flight requests, pong
   waiters, subscribers, etc. at compile time) and turning the dyn-trait transport/executor
   abstraction into a generic-parameter one instead - which cascades into `Client<E>`'s
   single-generic design (see CLAUDE.md's `no_std`+`alloc` section for why the dyn-trait
   approach was chosen). Not expected to move throughput - OCPP's request rate is low enough
   that heap traffic here is nowhere near a hot path - the payoff would be deterministic
   worst-case latency, no fragmentation risk over long uptimes, and static memory budgeting for
   genuinely allocator-less embedded targets. No specific target driving this yet; tracked as a
   future goal.
