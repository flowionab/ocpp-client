# Production readiness

Gaps to close before pointing real hardware/fleets at this crate, in priority order.

**Scope note.** The core crate over its WebSocket transport has been exercised against a real
CSMS. The remaining "not run against a real CSMS / real hardware" caveats below and in the
per-crate READMEs are specifically about the **embedded** satellite crates
(`ocpp-transport-embassy-net`, `ocpp-board-stm32h723-nucleo`) - don't read them as applying to
`ocpp-client` itself.

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
   the callback fires after a redial and not before). Where the redial *goes* is also a
   caller's choice now: `ConnectOptions::reconnector` overrides the built-in
   fixed-address `Reconnector`, and `websocket_transport(address, version, options)` opens the
   transport halves such a reconnector must return, so pointing a live connection at a different
   CSMS address does not mean reimplementing this crate's WebSocket plumbing - or dropping the
   `Client` and losing every handler, in-flight request and queued message with it. That is what
   an OCPP 2.x `SetNetworkProfile` switch needs from this layer. Covered by
   `tests/ocpp_1_6_custom_reconnector.rs` (a real drop on one address, redialled onto a second
   address the initial connect never saw, plus `ReconnectBehavior::Disabled` still winning over a
   supplied reconnector).

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

5. **Versioning/release.** Crate is `0.5.0`; `0.4.0` is the newest on crates.io. The
   git-fork dependency blocker is long gone - `ocpp-types` is a real crates.io release (`0.3.0`),
   not a git dependency. `ocpp-types` itself is still early (`0.3.x`, same org as the old
   `rust-ocpp` fork - see `MIGRATION_OCPP_TYPES.md`'s Risk section for a codegen bug found and
   fixed upstream mid-migration), so pin it deliberately rather than assuming API stability - its
   0.2.0 was itself a breaking release, and the sections at the bottom of that file record what
   each bump cost here. 0.3.0 cost nothing: purely additive upstream, so this crate's 0.5.0 is a
   dependency bump with no source change. This crate's own API is pre-1.0 and still moving: 0.3.0
   carried breaking changes to `TransportSink`/`TransportStream`, `ReconnectPolicy` and
   `ConnectOptions`' defaults, and 0.4.0 changes every `dateTime` field's type and adds a
   `customData` type parameter to the 2.x markers. 0.5.0 breaks nothing in this crate's own API -
   it is a minor bump only because `ocpp-types` is re-exported (`pub use ocpp_types;`), so a
   consumer naming that crate directly has to move 0.2 → 0.3 alongside it.

   **Settled in 0.5.0:** `ocpp-types` 0.3.0's `validate` feature is forwarded under the same
   name, off by default, and each version's error type gained `From<ValidationError>`.

   Validation is **not** wired into `Client::call`, and that is the decision worth recording
   rather than the feature itself. Three reasons, in order of weight:

   1. **The bound is not additive.** `Client::call` can only validate if `A::Request` carries a
      `Validate` bound. Unconditionally, that forces the feature on for everyone and makes
      `Validate` mandatory for every custom `Action`. Behind `#[cfg(feature = "validate")]`, the
      `Action` trait changes shape with a feature flag - and because cargo features unify across
      the whole graph, one crate enabling it would break an unrelated crate's custom `Action`
      impl, which that crate's author could only fix by implementing a trait they never asked
      for. `src/action.rs` advertises `Action` as open to consumers, so this taxes the documented
      extension path to serve the built-in one.
   2. **`ClientError` would grow 64 → ~304 bytes.** `ValidationError` is 296 bytes: it holds a
      16-segment path inline, by design, because a path that names the field is the whole point.
      Every `Result<Response, ClientError<E>>` in the public API would carry that, and clippy's
      `result_large_err` would fire across the surface. (Both figures measured, not estimated.)
   3. **The payoff is asymmetric.** A charge point mostly *builds* messages, from bounded fields
      the types already make unrepresentable. What `validate` adds outbound is over-long
      unbounded strings, empty arrays and out-of-range integers - mostly caller bugs that surface
      in development. Upstream's own docs aim the feature at whoever receives untrusted payloads,
      which is the CSMS.

   A caller who wants it writes `request.validate()?` before `client.call(..)`: one line, no cost
   to anyone else, and it composes with their own error type. Recursive validation with 296-byte
   stack frames is also not something to do unasked on a Cortex-M.

   **Open, small:** `src/client.rs`'s `on` answers *any* undecodable inbound payload with
   `not_implemented`, which is the wrong wire code - `FormationViolation` or
   `TypeConstraintViolation` fits a payload that failed to deserialize. Unrelated to `validate`
   (that path is a serde failure, not a constraint breach), but adjacent, and now that the
   constraint-violation codes are reachable the gap is more visible.

   Release metadata was tidied at the same time: `rust-version = "1.87"` (verified with
   `cargo +1.87 check --lib --all-features`; the binding constraint is `ocpp-types`, which
   declares the same 1.87, and building the *test suite* needs 1.88 for dev-dependencies), plus
   `categories`, `readme`, `documentation`, a `keywords` list that finally mentions 2.1, and an
   `exclude` that keeps `.idea/`/`.github/` out of the published tarball - `.idea/` had been
   shipping inside the crate through 0.2.2, and was also purged from git history.

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

   Scoping pass (no code changes yet) turned up two layers to this, not one:

   - **The collections are mechanical.** `BTreeMap<Uuid, OneShot<..>>` (pending responses),
     `BTreeMap<String, Chan<..>>` ×2 (request/notification handler registries), `VecDeque<OneShot<()>>`
     (pong waiters), `Chan<T>`'s internal `VecDeque<T>`, and `Vec<Arc<Signal>>` (ping/reconnect
     subscriber fan-out, in `src/sync.rs`) all become `heapless::FnvIndexMap`/`Deque`/`Vec` with
     const-generic capacities. The one real design question is a backpressure policy for `Chan`'s
     currently-unbounded queue (drop oldest / reject / block the sender) - bounded-by-construction is
     the point on embedded, so "unbounded" isn't an option to preserve. `Arc` itself (used so a
     `Client` clone handed to a spawned task shares state with the original, and so `OneShot`'s
     `Signal` can live in a lookup map while the caller independently awaits its own clone) needs a
     compile-time-sized slot pool referenced by index instead, or a redesign where handlers don't need
     independent ownership of shared state.
   - **The actual crux is task spawning, not collections.** `Executor::spawn` (`src/runtime.rs`) takes
     `Pin<Box<dyn Future<Output = ()> + Send>>`, and `Client` spawns a *new background task per call*
     to `on()`, `on_notification()`, `on_ping()`, `on_reconnect()`, plus the read loop itself. That's a
     heap-task model by construction. `embassy-executor` - the realistic no_std target - has no
     equivalent: task pools are statically sized at compile time via `#[embassy_executor::task]`, with
     no "spawn an arbitrary boxed future at runtime" escape hatch. Reaching true no-alloc means `Client`
     can't spawn per-handler tasks dynamically anymore - it needs to poll a fixed, compile-time-bounded
     set of registered handlers from one task instead (a hand-rolled `select` over an array of
     futures). That's a real change to `on()`/`on_notification()`'s semantics (how many concurrent
     handlers are supported becomes a compile-time constant), not just an internal swap - which is
     exactly the kind of decision that should be driven by a real embedded target's actual constraints
     rather than guessed at ahead of one.

   Separately (surfaced by the same pass, but independent of alloc): `TransportSink`/`TransportStream`'s
   method futures (`src/transport.rs`) are bounded `+ Send`, which is wrong for a single-threaded
   embedded executor where futures are never required to be `Send` (embassy tasks are commonly `!Send`
   - see the `getrandom`/`Spawner`/`Stack` gaps item 6 already documents for the embedded satellite
   crates). Dropping that bound isn't a local change, though: `Client`'s background read loop awaits
   those futures inside the async block handed to `Executor::spawn`, whose own signature requires
   `Pin<Box<dyn Future<Output = ()> + Send>>` (needed by `TokioExecutor::spawn` → `tokio::spawn`, which
   itself requires `Send`). Relax the transport bound alone and the default tokio/std build stops
   compiling - fixing it properly means reworking `Executor::spawn`'s `Send` bound too, which is really
   part of the same generic-parameter redesign as the rest of this item, not a separate one.

10. ~~**No client-initiated keepalive, and no `WebSocketPingInterval` to report.**~~ **Done.** The
    client only ever *replied* to pings the CSMS sent (`Client::send_ping` existed but nothing called
    it on a schedule), so a half-open connection - a dropped NAT entry, a mobile link gone without a
    FIN - was undetectable: the read loop stayed parked in `TransportStream::recv` until the OS TCP
    timeout, and the reconnect machinery from item 1 never got a chance to fire. Downstream consumers
    also had no interval to report for `OCPPCommCtrlr.WebSocketPingInterval` (2.0.1/2.1) or the 1.6
    security whitepaper's `WebSocketPingInterval` key, and were hardcoding `0`.

    `src/keepalive.rs` now holds `KeepalivePolicy`/`KeepaliveBehavior` (shaped like item 1's
    `ReconnectPolicy`/`ReconnectBehavior`), with the loop in `client.rs`. It lives in the engine, not
    the WebSocket transport, so `ocpp-transport-embassy-net` gets it for free. `ConnectOptions`
    defaults it to **enabled** at 60s tolerating one missed pong; `ClientConfig::new` (the
    raw-transport path) defaults to disabled. After `max_missed` unanswered pings the loop calls
    `Client::force_reconnect`, which the read loop honors only when a reconnector is configured -
    otherwise dropping the connection would leave a permanently deaf client, strictly worse than an
    unanswered ping. `Client::ping_interval`/`set_ping_interval` are the non-`async` read/write path
    for the spec variable, and a write applies immediately instead of after the interval already being
    waited out.

    Two latent bugs had to be fixed first, both near-unreachable while pings were only ever manual and
    both routine once a timer sends them: pongs were matched to pings **positionally**, so (a) a ping
    that timed out left its waiter queued forever and permanently offset every later ping's pong by
    one - poisoning every subsequent `send_ping` on that client, reachable via reconnect since the
    waiter table wasn't cleared on redial - and (b) an unsolicited pong (legal per RFC 6455) caused the
    same desync. Pings now carry an 8-byte correlation token as their payload and only a pong echoing
    it resolves them, which is why `TransportSink::ping`/`pong` and `TransportEvent::Ping`/`Pong` grew
    `Vec<u8>` payloads (breaking, for transport implementors only). Racing `recv` against the
    force-reconnect signal also makes cancel-safety a documented contract on `TransportStream::recv`.

11. ~~**Nothing caught actions whose types existed but were never wired up.**~~ **Done.** Five actions
    shipped in 0.2.0 with `ocpp-types` request/response types present but no `ocpp_*_action!`
    invocation, so callers had no way to send or receive them; it surfaced only as a downstream bug
    report, and was fixed in 0.2.1. Nothing about it was visible at compile time - an unwired type is
    just an unreferenced one. `tests/action_coverage.rs` now locates the `ocpp-types` source via
    `cargo metadata`, reads the `const ACTION` string out of each `*_request.rs`, and fails the build
    naming any action with no matching macro invocation. All three versions are fully covered today
    (28 / 64 / 91). The repo also gained a `CHANGELOG.md` - it had none, so which release added which
    action was invisible to anyone not reading git log, which is what let the original report be filed
    against the wrong version.

12. ~~**`disconnect()` raced the reconnector.**~~ **Done.** Closing the transport looks exactly like
    a dropped connection from the read loop's side, so with reconnect enabled (the default on
    `ConnectOptions` since item 1) an explicit `disconnect()` produced an EOF the reconnector
    redialled - meaning on default options there was no way to stop a client at all. `Client` now
    carries a sticky `closed` flag that `disconnect()` sets *before* closing the transport, and that
    outranks every automatic recovery path: the read loop exits rather than redialling (whether it
    was parked in `recv` or saw the EOF), the keepalive task returns, `set_ping_interval` can't
    restart it, `force_reconnect()` is a no-op, and further sends fail fast with
    `ClientError::Closed` - the first construction site that variant has ever had. `LoopExit`
    (`Eof`/`Forced`/`Shutdown`) is what makes the read loop's three exit paths distinguishable
    instead of all collapsing into "connection ended, redial". Covered by
    `tests/ocpp_1_6_disconnect.rs`, whose real-socket case is the one that actually reproduced the
    race - the in-memory fake's `close()` doesn't end the peer's stream, so the fake never produced
    the EOF that triggered the redial.

    Racing `recv` against the wake signal is now unconditional (it used to be armed only when a
    reconnector existed), so `disconnect()` can pull the read loop out of a `recv` that would
    otherwise park until the OS TCP timeout. That is why `TransportStream::recv`'s cancel-safety
    contract applies to every transport, not just ones used with reconnect.

13. ~~**Successful-connect-then-immediate-EOF was a hot reconnect loop.**~~ **Done.**
    `ReconnectPolicy` only delayed *failed* connect attempts, and `attempt` reset to `0` on every
    success - so a peer that accepts the connection and then closes it immediately (a CSMS rejecting
    the charge point at the application layer, an overloaded or misconfigured endpoint, a load
    balancer with no live backend) got redialled with no delay at all, unboundedly. Measured against
    a local server that completes the WebSocket handshake and drops: **~9,900 connections in 2
    seconds**, i.e. ~5k/s sustained from a single charge point. A charge point behaving that way
    against a real CSMS is a self-inflicted DoS, and the fleet-wide version is worse.

    Surfaced while testing item 12 and confirmed independently against the fixed code, so it was
    neither caused nor fixed by that change.

    The fix keeps "reconnect promptly when the link genuinely comes back" - the behavior worth
    keeping - by separating *a dial completed* from *a connection works*: `attempt` now lives across
    connections and is reset only by inbound traffic arriving on one (`attempt = 0` in the read
    loop's inbound-event arm), never by a dial merely succeeding. The delay also moved to *before*
    each dial rather than only after a failed one, since the old ordering meant a
    succeed-then-instantly-drop cycle never waited at all. A connection that carries traffic
    therefore still costs exactly one `initial_delay` on its next drop, while one that never carries
    anything escalates to `max_delay`.

    `ReconnectPolicy::jitter` (new, default `true`) spreads each delay uniformly over
    `[delay / 2, delay]`, because a fleet that all lost the same CSMS previously retried in lockstep
    and hit it as a thundering herd on recovery. Half the delay stays un-jittered so a randomly tiny
    value can't defeat the rate bound. Randomness comes from a throwaway v4 UUID - `uuid` is already
    a dependency for message ids and its RNG already works on `thumbv7em-none-eabihf`, so this
    needed no new dependency and no new embedded plumbing; the arithmetic is integer-only to avoid
    a float dependency on no-FPU targets.

    Two knock-on behavior changes, both documented in CHANGELOG.md: the first redial after a drop
    now waits `initial_delay` (jittered) instead of going out immediately, and `disconnect()` during
    a backoff no longer has to wait out the remaining delay. Covered by
    `tests/ocpp_1_6_reconnect_backoff.rs`; three of its four cases fail against the old logic, the
    fourth being the guard against over-correcting (escalation must not punish a connection that
    genuinely worked).

14. ~~**Unbounded bookkeeping growth and leaked handler tasks.**~~ **Done.** Three leaks in the same
    family, all invisible from outside the crate - they show up only as memory growth on a charge
    point that has been up for weeks:

    - **`pending_responses` leaked on every timed-out request.** `do_send_request` inserted a waiter
      keyed by message id and only `handle_frame` ever removed one, on an arriving
      CALLRESULT/CALLERROR. A request that timed out - or that failed to reach the wire at all -
      left its entry forever. Each request now cleans up after itself on every exit path. This is
      the identical failure the pong table had (item 10); fixing that one and not this one was an
      oversight, and both tables are now covered by the same suite so neither regresses alone.
    - **`Client::on`/`on_notification` leaked a task per re-registration.** Overwriting the map entry
      made the previous handler unreachable but not finished - it stayed parked forever on a channel
      nothing could deliver to. `Chan` (`src/sync.rs`) gained `close()`, and the superseded handler
      is retired: it drains what was already dispatched to it, answers those calls, then exits.
      Draining before stopping is deliberate - dropping the queue would leave a peer with no reply.
    - **`wait_for` never unregistered** (`test` feature). The action stayed bound to a reader-less
      channel, so a later CALL for it was queued and silently forgotten: no CALLRESULT, no CALLERROR,
      indistinguishable from a hung client. It now removes its registration on the way out, and only
      when that registration is still its own.

    Covered by `tests/ocpp_1_6_bookkeeping.rs`, using two new `test`-feature accessors
    (`pending_request_count`, `pending_ping_count`) and a drop-detecting callback to prove a retired
    task actually ended. Each case was confirmed to fail against the unfixed code.

    Still open in this area, deliberately: `disconnect()` does not fail *in-flight* requests early -
    they wait out their timeout rather than returning `Closed` immediately. That is latency, not a
    leak. Doing it properly means either changing what the pending-response channel carries or
    racing each waiter against a connection-generation signal, and neither is worth the complexity
    until something needs it.
