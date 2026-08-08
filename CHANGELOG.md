# Changelog

Notable changes per release. Dates are release dates on crates.io.

This file starts at 0.2.0; earlier releases (0.1.x, a per-version tokio-hardwired design that
predates the current generic engine) are covered by git history only.

## 0.5.0 - 2026-08-08

Tracks `ocpp-types` 0.3.0. A dependency-only release: no source file in `src/` changed, and the
whole test suite passed against the new version unedited.

### Changed

- **BREAKING (dependency): `ocpp-types` updated to 0.3.0.** Upstream's own release is purely
  additive - no message type, field type or action changed shape, and all 39 / 64 / 91 actions
  are the same ones - so **no code in this crate needed editing and no call site here does
  either**. It is called out as breaking only because `ocpp-types` is a *public* dependency
  (`pub use ocpp_types;`): a consumer that also names `ocpp-types` in its own `Cargo.toml` must
  move from `0.2` to `0.3` in the same step, or the two copies are distinct types to the
  compiler. A consumer that reaches the types only through `ocpp_client::ocpp_types::..` - the
  spelling `src/lib.rs` recommends precisely so this stays a non-event - needs to change nothing.

### Added

- **A `validate` feature**, forwarding `ocpp-types`' own. That adds a `Validate` trait to every
  message type covering the spec constraints the types cannot carry: `maxLength` on fields too
  large to inline as a `heapless::String` (certificates, CSRs, OCSP results), plus every
  `minItems`, `minimum`, `maximum` and `multipleOf` in the schemas. Off by default.
- **`From<ValidationError>` for `OCPP1_6Error`, `OCPP2_0_1Error` and `OCPP2_1Error`** (same
  feature). `ocpp-types` classifies a violation as a property- or occurrence-class breach but
  leaves the wire code to the caller's version, and the versions disagree: OCPP 1.6J's RPC error
  table spells it `OccurenceConstraintViolation`, missing an `r` that 2.0.1 and 2.1 restored.
  These impls pick the right one, use upstream's rendering (which names the failing field) as the
  CALLERROR description, and put the JSON path in `errorDetails` under `"path"` so a peer can
  match on it without parsing prose. A handler can now reject a bad payload with
  `request.validate()?` and get the correct code on the wire.
  See `tests/validation_error_mapping.rs`.

  **Validation is never automatic.** It is deliberately not wired into `Client::call`: that would
  require a `Validate` bound on `Action::Request`, and a trait bound that appears only when a
  feature is enabled is not additive - one crate in the graph enabling `validate` would break an
  unrelated crate's custom `Action` impl, which `src/action.rs` documents as a supported
  extension point. Callers validate explicitly, which is one line and composes with their own
  error type. The rationale is recorded in PRODUCTION_READINESS.md item 5.

## 0.4.0 - 2026-08-08

Tracks `ocpp-types` 0.2.0. Everything below follows from that release; the engine itself
(`src/client.rs`, `envelope.rs`, `transport.rs`) needed no changes at all.

### Added

- **The eleven OCPP 1.6 security-whitepaper actions**, which `ocpp-types` 0.2.0 was the first
  release to define: `CertificateSigned`, `DeleteCertificate`, `ExtendedTriggerMessage`,
  `GetInstalledCertificateIds`, `GetLog`, `InstallCertificate`, `LogStatusNotification`,
  `SecurityEventNotification`, `SignCertificate`, `SignedFirmwareStatusNotification` and
  `SignedUpdateFirmware`. 1.6 now wires up 39 actions rather than 28; 2.0.1 (64) and 2.1 (91) are
  unchanged. Each gets the usual `send_*`/`on_*`/`wait_for_*` trio, and
  `tests/ocpp_1_6_security_actions.rs` covers one round trip apiece.
- **2.x action markers take the `customData` type as a parameter**: `Reset<AcmeExtension>`,
  `Heartbeat<NoCustomData>`, and so on, defaulting to the specification's `CustomData`. A
  deployment with a vendor extension richer than a bare `vendorId` can now read and write it
  through `Client::call`/`Client::on` without hand-writing an `Action` impl. See
  `tests/custom_data_generics.rs`.
- **A `chrono` feature**, forwarding to `ocpp-types`' own: `From`/`Into` between `OcppTimestamp`
  and `chrono::DateTime` for callers that already keep time in chrono. Off by default, and chrono
  never touches the wire path.

### Changed

- **BREAKING: every `dateTime` field is now `ocpp_types::OcppTimestamp` instead of `String`.**
  Construct them with `OcppTimestamp::parse_rfc3339(..)` (or `From<chrono::DateTime>` under the
  new `chrono` feature) and compare against a parsed value rather than a string literal. Beyond
  the type, this changes behaviour twice over: the client now *validates* what the CSMS sends, so
  a malformed `dateTime` surfaces as `ClientError::Decode` where it used to be handed through as a
  string; and the value is an instant, so two timestamps naming the same moment in different UTC
  offsets compare equal. Fractional seconds and non-UTC offsets both survive the round trip -
  `tests/ocpp_1_6_timestamps.rs` pins all of it.
- **BREAKING: 2.0.1 and 2.1 message types carry a `customData` type parameter.** The generated
  `send_*`/`on_*`/`wait_for_*` methods stay concrete at `CustomData`, so ordinary call sites are
  unaffected, but a 2.x request or response built in a `let` binding with no expected type now
  needs an annotation (`let request: ResetRequest = ResetRequest { .. }`) - a defaulted type
  parameter does not participate in inference. The methods were deliberately left non-generic for
  exactly this reason; the marker types carry the parameter instead.
- **BREAKING: `ocpp_types::v16::common::RequestedMessage` is now
  `TriggerMessageRequestRequestedMessage`**, renamed upstream to make room for
  `ExtendedTriggerMessageRequestRequestedMessage`. A pure rename, no variants changed.
- **BREAKING: 48 2.x string fields whose length the specification leaves to a configuration
  variable are now `String` instead of `heapless::String<N>`** - certificates, certificate
  chains, CSRs, OCSP results, `MessageContent.content` and the like. Construction drops from
  `"...".try_into().unwrap()` to `"...".into()`. In the other direction, 89 `dateTime` fields and
  8 more 2.1 date/time-of-day fields (`TariffConditions.start_time_of_day` and friends) left
  `String` for `OcppTimestamp`/`OcppDate`/`OcppTimeOfDay`.
- `ocpp-types` updated to 0.2.0. Its `alloc` feature is now upstream's default; this crate still
  names its features explicitly, so nothing about the build changes.

## 0.3.0 - 2026-08-08

### Added

- **WebSocket keepalive.** The client can now ping the CSMS on a schedule and, when it stops
  answering, force the connection to be redialled. Previously the client only ever *replied* to
  pings the server sent, so a half-open connection - a dropped NAT entry, a mobile link that went
  away without a FIN - was invisible: the read loop stayed parked in `recv` until the OS TCP
  timeout, and the reconnect logic added in 0.2.0 never got a chance to fire.
  - `KeepalivePolicy { interval, timeout, max_missed }` and `KeepaliveBehavior::{Enabled,
    Disabled}`, shaped after the existing `ReconnectPolicy`/`ReconnectBehavior` pair.
  - `ConnectOptions::keepalive`, **defaulting to enabled** at a 60-second interval tolerating one
    missed pong. See "Changed" below.
  - `Client::ping_interval()` and `Client::set_ping_interval()` - the read/write path for OCPP's
    `OCPPCommCtrlr.WebSocketPingInterval` (2.0.1/2.1) and the 1.6 security whitepaper's
    `WebSocketPingInterval` configuration key. Both are non-`async` so a `GetVariables` handler can
    call them directly, and `set_ping_interval` takes effect immediately rather than after the
    interval already being waited out expires. This is what previously forced consumers to report a
    hardcoded `0`: the library owned no interval to report.
  - `Client::force_reconnect()` - abandon the current transport and redial now, for callers with
    their own liveness signal. This is what keepalive escalates to after `max_missed` unanswered
    pings. No-op without a configured reconnector.
  - `ClientConfig` and `Client::from_transport_with_config`, so options live in a struct instead of
    a positional parameter list. `from_transport_with_reconnect` had already reached seven
    arguments; keepalive would have made it eight.
- `Client::pending_request_count()` / `Client::pending_ping_count()` behind the `test` feature -
  instrumentation for the bookkeeping tables, since a leak in either is otherwise invisible from
  outside.
- `Client::is_closed()` - whether `disconnect()` has been called. Deliberately does *not* report
  transient connection state: that would be stale the moment it returned, and `on_reconnect` is the
  reliable way to observe reconnection.
- **`tests/action_coverage.rs`** fails the build if `ocpp-types` defines an action that no
  `ocpp_*_action!` invocation wires up. This is the class of gap that shipped in 0.2.0 (see below)
  and was invisible at compile time.

### Fixed

- **Every timed-out request leaked an entry in the pending-response table.** `do_send_request`
  registered a waiter keyed by message id, and the only code that removed it was the read loop on an
  arriving CALLRESULT/CALLERROR - so a request that timed out, or that never made it onto the wire,
  left its entry behind permanently. On a charge point running for weeks against an intermittent
  CSMS the map grew without bound. Each request now removes its own waiter on every exit path, the
  same fix the ping table already got.

  In-flight requests are still not failed early by `disconnect()`; they wait out their timeout. That
  is a latency wart rather than a leak, and is left as-is.
- **Replacing a handler leaked its task.** `Client::on` (and `on_notification`) overwrote the map
  entry for an action, which made the previous task unreachable but not finished - it stayed parked
  forever on a channel nothing could deliver to, one leaked task per re-registration. The internal
  channel is now closable, and the superseded handler is retired: it drains whatever was already
  dispatched to it, answers those calls, and then exits.
- **`wait_for` left the action registered** (`test` feature). After it returned, the action stayed
  bound to a channel with no reader, so any later CALL for it was queued and silently forgotten -
  the peer got no CALLRESULT and no CALLERROR, which is indistinguishable from the client hanging.
  It now unregisters on the way out, and only if the registration is still its own.
- **A peer that accepted the connection and then closed it immediately was redialled in a hot loop
  with no delay at all.** `ReconnectPolicy` only delayed *failed* dials, and the attempt counter
  reset on every successful one - so a dial that completed and then instantly dropped never waited,
  unboundedly. Measured against a local server that completes the WebSocket handshake and drops:
  **~9,900 connections in 2 seconds**, roughly 5k/s from a single charge point. The triggers are
  ordinary - a CSMS rejecting the charge point at the application layer, an overloaded endpoint, a
  load balancer with no live backend - and a fleet doing this at once is a self-inflicted DoS.

  Two changes fix it. The backoff delay is now applied *before* every dial rather than only after a
  failed one, and the attempt counter is reset by evidence that a connection actually **works** -
  any inbound traffic on it - instead of by the mere fact that a dial completed. So a genuine
  transient drop still reconnects after one `initial_delay`, while a connection that never carries
  anything escalates to `max_delay` as intended.

  `disconnect()` during a long backoff now also takes effect immediately instead of waiting out the
  remaining delay (up to `max_delay`, 60s by default).
- **Reconnect delays are now jittered** (`ReconnectPolicy::jitter`, default `true`): each delay is
  drawn uniformly from `[delay / 2, delay]`. Without it, every charge point that lost the same CSMS
  retried in lockstep, so the endpoint coming back got the whole fleet at once, repeatedly. Half the
  delay is left un-jittered so a randomly tiny value can't defeat the rate bound. Randomness comes
  from `uuid`, already a dependency and already working on this crate's bare-metal target, so no new
  RNG dependency or embedded plumbing is involved.

  One behavior change worth noting: the first redial after a drop now waits `initial_delay`
  (jittered) where it previously went out immediately.
- **`disconnect()` was undone by the reconnector.** Closing the transport is indistinguishable from
  a dropped connection from the read loop's side, so with reconnect enabled - the default on
  `ConnectOptions` - an explicit `disconnect()` produced an EOF that got dutifully redialled. On
  default options there was no way to stop a client at all.

  `disconnect()` now marks the client closed before closing the transport, and that flag is sticky
  and outranks every automatic recovery path: the read loop exits instead of redialling (whether it
  was parked in `recv` or saw the EOF), the keepalive task stops, `set_ping_interval` cannot restart
  it, `force_reconnect()` becomes a no-op, and further `call`/`send_*`/`send_ping` return
  `ClientError::Closed` immediately instead of writing to a dead transport and waiting out the
  request timeout. `disconnect()` is also idempotent now. An *unrequested* drop is still redialled
  exactly as before.

  This also gives `ClientError::Closed` its first construction site - the variant existed but was
  never produced by anything.

### Changed

- **Ping/pong now carry payloads, and pongs are matched by correlation token.** `send_ping` writes
  an 8-byte token as the ping's application data and only a pong echoing that exact payload
  resolves it, per RFC 6455 §5.5.2-3. Matching was previously positional, which had two failure
  modes that a scheduled keepalive would have hit constantly:
  - A ping that timed out left its waiter queued forever, permanently offsetting every later
    ping's pong by one - so *every* subsequent `send_ping` on that client timed out. Reachable
    before this release via reconnect, since the waiter table was not cleared on redial and stale
    waiters ate the first pong of the new connection.
  - An unsolicited pong (which RFC 6455 permits) caused the same one-off desync.

  Outstanding pings are now also cleared when the read loop swaps in a reconnected transport.
- **`ConnectOptions::default()` enables keepalive**, where the previous behavior sent no
  unsolicited traffic. Same reasoning as `reconnect` defaulting to enabled: without keepalive a
  charge point cannot detect a half-open link at all. Set `keepalive:
  KeepaliveBehavior::Disabled` to restore the old behavior. The lower-level
  `Client::from_transport*` constructors still default to **disabled** - a caller assembling a
  client from raw transport halves has said nothing about wanting background traffic on it.

### Breaking

- **`rust-version = "1.87"` is now declared.** Not a change in what the crate needs, but if you
  were building it on an older toolchain it will now fail with a clear MSRV error rather than a
  confusing parse error. Contributors need 1.88 for the test suite (dev-dependencies).
- `ReconnectPolicy` gained a `jitter: bool` field, which breaks struct-literal construction
  (`ReconnectPolicy { initial_delay, max_delay, multiplier }`). Add `jitter: true` for the new
  default behavior, `jitter: false` for exact delays, or switch to
  `..ReconnectPolicy::default()`.

The remaining changes affect implementors of the transport traits only - not callers of
`connect_*`/`Client`. The in-tree `ocpp-transport-embassy-net` is updated accordingly.

- `TransportSink::ping` and `TransportSink::pong` now take a `Vec<u8>` payload, which must be
  transmitted verbatim (`pong` echoes the triggering ping's payload).
- `TransportEvent::Ping` and `TransportEvent::Pong` are now `Ping(Vec<u8>)` / `Pong(Vec<u8>)`.
- `TransportStream::recv` is now documented as having to be **cancel-safe**: its future may be
  dropped mid-poll without losing an event. The read loop races it against the force-reconnect
  signal, which is how a stalled `recv` gets abandoned instead of parking the loop. Both in-tree
  implementations already satisfied this (`futures::StreamExt::next`, an `embassy-net` socket
  read); a third-party transport buffering partial state across an `.await` in a local needs to
  move that state into `self`.

## 0.2.2 - 2026-08-08

### Added

- `ConnectOptions::reconnector` - lets callers decide where a dropped connection is redialled, for
  a charge point that must move to a different CSMS address (an OCPP 2.x network connection
  profile, a failover endpoint) without tearing down its `Client` and losing its handlers,
  in-flight requests and queued messages.
- Criterion benchmarks (`benches/`) plus a CI job that compiles them.
- Community/repo health files: code of conduct, security policy, PR and issue templates.

## 0.2.1 - 2026-08-06

### Added

- **Five actions whose types existed in `ocpp-types` but which no macro invocation had wired up**,
  so callers could not send or receive them. If you are on 0.2.0 and believe one of these is
  missing, upgrade - this is the release that added them:
  - OCPP 2.0.1: `SecurityEventNotification`
  - OCPP 2.1: `TriggerMessage`, `SetDisplayMessage`, `GetDERControl`, `SetDERControl`,
    `UpdateDynamicSchedule`

  Each has a fake-transport round-trip test. `tests/action_coverage.rs` (0.3.0, above) now
  prevents this class of gap from recurring.

### Changed

- `ocpp-types` updated to 0.1.3.

## 0.2.0 - 2026-08-04

First release of the rewritten crate: one generic engine (`Client<E>`) shared by every OCPP
version, replacing the previous per-version, tokio-hardwired design.

### Added

- OCPP 1.6, 2.0.1 and 2.1, all in `default` features.
- Automatic reconnection with bounded exponential backoff (`ReconnectPolicy`,
  `ReconnectBehavior`), plus `Client::on_reconnect` for post-redial session resync.
- `no_std` + `alloc` support: the engine no longer hard-depends on tokio or `async-trait`. Task
  spawning and timeouts go through the `Executor`/`Timer` traits; `tokio-runtime` provides the std
  implementations.
- Custom TLS trust config via `ConnectOptions::tls_config`, including client certificates for
  OCPP Security Profile 3 (mutual TLS).
- `connect()` for protocol negotiation across every compiled-in OCPP version.
- Structured logging through `tracing` (no global subscriber is installed by this crate).
- `SEND` (OCPP-J 2.1 message type 6) support, and `NotifyPeriodicEventStream` modeled as a
  fire-and-forget notification rather than a call/response pair.
- Embedded satellite crates: `ocpp-transport-embassy-net` and
  `ocpp-board-stm32h723-nucleo`. Neither has been run against real hardware or a real CSMS yet.

### Changed

- Message types migrated from a `rust-ocpp` fork to `ocpp-types` on crates.io. See
  `MIGRATION_OCPP_TYPES.md`.
