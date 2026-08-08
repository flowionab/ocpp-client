# Roadmap: WebSocket keepalive / ping interval

> **Status: all five phases implemented.** See CHANGELOG.md's Unreleased section for the shipped
> surface. Deviations from the plan below, all noted in place: phase 0's `MockTimer` was dropped in
> favour of millisecond intervals with generous bounds (simpler, and the tests are deterministic
> without it); phase 5's coverage check turned out to be exactly implementable via `cargo metadata`
> plus `ocpp-types`' `const ACTION`, rather than the "optional but cheap" approximation expected
> here. Kept as the record of why the work is shaped the way it is.

Triage and plan for three gaps reported by a downstream agent consuming `ocpp-client 0.2.1`. The
short version: **one of the three is real, two were already fixed in 0.2.1 itself** and the
reporter was almost certainly reading 0.2.0. That mis-triage is itself worth an action item, so
it's tracked below as item 5.

## Triage

| # | Report | Verdict |
|---|--------|---------|
| 1 | No ping-interval on `ConnectOptions`; transport only replies to pings | **Real.** Confirmed at HEAD (0.2.2) |
| 2 | Four 2.1 actions never generated (`GetDERControl`, `SetDERControl`, `SetDisplayMessage`, `UpdateDynamicSchedule`) | **Already fixed.** Shipped in 0.2.1 |
| 3 | `SecurityEventNotification` missing for 2.0.1 | **Already fixed.** Shipped in 0.2.1 |

### Why 2 and 3 are already closed

Commit `2c93e83` "Wire up six action wrappers already supported by ocpp-types 0.1.2" is an
*ancestor* of `70449f4` "Bump to 0.2.1" — it added exactly these five actions (plus 2.1
`TriggerMessage`). Verified three ways:

- `git show 70449f4:src/ocpp_2_1/actions.rs` contains all four 2.1 macro invocations
  (lines 395, 813, 822, 930), and `70449f4:src/ocpp_2_0_1/actions.rs` line 526 has
  `SecurityEventNotification`.
- The **published** 0.2.1 tarball from `static.crates.io` contains the same — 91
  `ocpp_2_1_action!` invocations, all five present.
- The published **0.2.0** tarball has 86 invocations and is missing all five.

So the reporter's environment was on 0.2.0 while believing it was 0.2.1. The available methods
today are `send_get_der_control` / `send_set_der_control` / `send_set_display_message` /
`send_update_dynamic_schedule` / `send_security_event_notification`, each with matching
`on_*` and `wait_for_*`.

**Action:** tell the consumer to bump to `0.2.2` and re-check. No library work needed for their
B6, B8.2, B2.6 or F4.4 items. (Their plan IDs — A4, B6, B8.2, B2.6, F4.4 — aren't resolvable from
this repo; the mapping above is by feature, not by ID.)

## The real gap: no client-initiated keepalive

Current state, all verified in source:

- `Client::send_ping()` (`src/client.rs:311`) sends **one** ping and awaits a pong, bounded by
  `self.timeout`. It's manual — nothing calls it on a schedule.
- `Client::on_ping()` (`src/client.rs:327`) observes *inbound* pings. The read loop auto-pongs
  every inbound `TransportEvent::Ping` (`src/client.rs:136`).
- `ConnectOptions` (`src/connect.rs:19`) has `username`/`password`/`timeout`/`reconnect`/
  `tls_config`/`reconnector`. No ping interval, and no getter to report one.
- Consequence: a dead-but-not-closed TCP connection is never detected. The read loop only leaves
  its inner loop when `recv()` returns `Ok(None)`/`Err(_)`, so a half-open socket parks there
  until the OS TCP timeout — the reconnect machinery added in 0.2.x never fires.

So the reporter's "`WebSocketPingInterval` reads 0, which is the honest answer" is exactly right:
there is nothing to report because the library doesn't own an interval.

### Two latent bugs this work must fix first

Both are pre-existing and currently near-unreachable because nothing pings on a timer. An
automatic keepalive loop makes both live, so they are prerequisites, not follow-ups.

**(a) Timed-out pings permanently desync the pong FIFO.** `send_ping` pushes a `OneShot` onto
`pong_waiters` and then races `waiter.wait()` against the timeout. On timeout it returns
`Err(Timeout)` **without removing its own waiter**, and the read loop's `Pong` arm only ever
`pop_front()`s. So once a pong is genuinely never delivered, the deque is offset by one forever:
the next pong signals the stale waiter, that ping times out too, and so on. Every subsequent
`send_ping` on that client fails. Reachable today via reconnect — `pong_waiters` is not cleared
when the read loop swaps in a new transport, so stale waiters survive the redial and eat the
first pong of the new connection.

**(b) Ping/pong are uncorrelated.** `WebSocketSink::ping` sends an empty payload
(`src/transport/websocket.rs:33`) and `TransportEvent::Pong` carries none, so matching is purely
positional. An unsolicited pong from the peer (permitted by RFC 6455) causes the same one-off
desync as (a). Exact fix is `ping(payload)` + `TransportEvent::Pong(Vec<u8>)` and matching on the
echoed payload — a breaking change to the two transport traits, and the only way to make
correlation actually correct.

### Zero test coverage today

`grep -rn ping tests/` hits only the fake transport's own `ping`/`pong` impls. There is no test
for `send_ping`, `on_ping`, or auto-pong. Per CLAUDE.md's mandatory-TDD rule, scaffolding comes
before behavior.

The harness is *nearly* ready: `tests/common/mod.rs`'s `FakeSink::ping` forwards a `Ping` event to
the peer end, and the peer's read loop auto-pongs — so a loopback pair produces real pongs, and a
test holding the peer's `FakeSource` can assert `TransportEvent::Ping` arrives. What's missing is a
controllable clock; see phase 0.

## Plan

Five phases, each independently shippable and each landing with tests. Sizes are relative, not
calendar estimates.

### Phase 0 — Test scaffolding (small)

- Add a `MockTimer` to `tests/common/mod.rs`: an `ocpp_client::Timer` impl whose `delay` resolves
  only when the test advances it, so interval scheduling can be asserted exactly rather than
  slept through. Without it every keepalive test pays real wall-clock time and turns flaky under
  CI load.
- Add the missing baseline tests for existing behavior: `send_ping` resolves on pong, times out
  without one, and the read loop auto-pongs an inbound ping.
- Add the two **failing** regression tests for bugs (a) and (b), so phase 1 has a red bar to turn
  green.

### Phase 1 — Fix pong correlation (small, unblocks everything)

- Remove the caller's own waiter from `pong_waiters` on the timeout path.
- Clear `pong_waiters` when the read loop swaps in a reconnected transport (same place
  `read_sink` is replaced, `src/client.rs:159`).
- Decide on payload correlation (see Decisions). If yes, this is the moment to change
  `TransportSink::ping` and `TransportEvent::Pong` — before a keepalive loop depends on the
  current shape.

### Phase 2 — Keepalive in the engine (medium)

Belongs in `Client<E>`, not in the WebSocket connect path: the transport traits already carry
`ping`/`pong`, so `ocpp-transport-embassy-net` and any future framed transport get keepalive for
free. It also survives reconnects for free — `send_ping` goes through `self.sink`, which the read
loop swaps in place, so a task spawned once keeps working across redials.

- New `src/keepalive.rs`, mirroring `src/reconnect.rs`'s shape:
  `KeepalivePolicy { interval, timeout: Option<Duration>, max_missed: u32 }` plus
  `KeepaliveBehavior::{Enabled(KeepalivePolicy), Disabled}`.
- Interval state is shared and mutable — `Arc<SharedMutex<Option<Duration>>>` on `Client` — so
  the value can be changed at runtime (phase 4) rather than frozen at construction.
- The loop itself needs no new sync primitive. `runtime::with_timeout(timer, interval,
  config_changed.wait())` is already exactly "sleep for `interval`, but wake early if
  signalled": `Err(Elapsed)` means tick, `Ok(())` means the interval was reconfigured. Reuse it
  instead of adding a `with_cancel`.
- `interval == 0` means disabled, matching how OCPP's `WebSocketPingInterval` reads.

Constructor pressure is the design problem here. `from_transport_with_reconnect` already takes
seven arguments and both constructors are public API; keepalive would make it eight and a third
constructor would make the set unmaintainable. Introduce
`ClientConfig { timeout, reconnector, reconnect_policy, keepalive }` and
`Client::from_transport_with_config(sink, stream, executor, timer, config)`, then reduce both
existing constructors to thin wrappers over it. Non-breaking, and it stops the parameter
explosion before the next option arrives.

### Phase 3 — Dead-peer detection (medium; this is what gives keepalive teeth)

Detecting a dead link is the whole point — logging a missed pong is not enough. After
`max_missed` consecutive misses, force a redial through the existing reconnect path.

There's no mechanism for that today, and the obvious one is insufficient:

- **Cooperative close alone won't do it.** Calling `sink.close()` from the keepalive task makes
  tungstenite emit a Close frame, and on a merely-idle socket the read half then returns
  `Ok(None)` and the reconnect path runs. But on a genuinely dead TCP link — the case keepalive
  exists to catch — that frame never lands and `recv()` stays parked. It's still worth doing so
  the peer sees a clean close when the link *is* alive, just not sufficient.
- **Recommended: race the read loop against a force-reconnect signal.** Have the read loop poll
  `stream.recv()` against a `Signal` using the same hand-rolled `poll_fn` pattern as
  `with_timeout`, and have the keepalive task fire that signal. Robust regardless of socket
  state, and no new dependency.
  - This makes cancel-safety a **contract** on `TransportStream::recv`: the future may be dropped
    mid-poll without losing a frame. Both current implementors already satisfy it
    (`StreamExt::next()` and tokio `mpsc::recv` are cancel-safe), but it must be documented on
    the trait, since third-party transports now have to honor it.
  - Wrap the `sink.close()` courtesy call in `with_timeout` so a dead socket can't hang the
    keepalive task.
- Emit `tracing` events on each miss and on the forced redial, consistent with the existing
  reconnect logging.

### Phase 4 — Expose it for device-model reporting (small)

This is what actually unblocks the reporter's A4. The library should own the interval and expose
it; it should **not** grow a device model — that belongs in the layer above (`ocpp-charge-point`).

- `Client::ping_interval() -> Option<Duration>` so a `GetVariables`/`GetConfiguration` handler can
  report the live value instead of hardcoding 0.
- `Client::set_ping_interval(Option<Duration>)` so a CSMS `SetVariables` on
  `OCPPCommCtrlr.WebSocketPingInterval` can take effect without tearing down the client. Signals
  the config-changed `Signal` from phase 2 so the running loop re-reads it immediately rather
  than after the current sleep expires.
- `ConnectOptions::keepalive: KeepaliveBehavior`, threaded through `connect_1_6` / `connect_2_0_1`
  / `connect_2_1` / `connect`. Add it to the redacting `Debug` impl (`src/connect.rs:57`).
- README: document the mapping to 2.x `OCPPCommCtrlr.WebSocketPingInterval` and the 1.6 security
  whitepaper's `WebSocketPingInterval` config key, so consumers wire the variable to the getter
  rather than reinventing a timer.

### Phase 5 — Make capability-vs-version legible (small)

The 0.2.0-reported-as-0.2.1 confusion cost a downstream consumer real planning effort, and the
repo currently gives them no way to check a claim like "action X is missing" against a version.

- Add a `CHANGELOG.md`. There is none, and `2c93e83`'s five new actions are invisible to anyone
  not reading git log.
- Have the README's protocol section state which version wired up which actions, or point at the
  changelog. It currently mentions none of the five by name.
- Optional but cheap: a test that asserts every action name in the relevant `ocpp-types` version
  module has a macro invocation, so "types exist but the wrapper doesn't" becomes a CI failure
  instead of a downstream bug report.

## Decisions needed

1. **Default for `ConnectOptions::keepalive`.** `ReconnectBehavior` defaults to `Enabled` on the
   reasoning that production charge points should keep retrying; the same argument applies to
   keepalive on NAT'd or mobile links, and a default of `Disabled` means most consumers never get
   dead-peer detection. Against: it changes on-wire behavior for existing 0.2.x users.
   *Recommendation:* `Enabled` at a conservative interval (60s), documented in the changelog as a
   behavior change. Pre-1.0 is the right time to take it.
2. **Payload correlation now or later (phase 1).** Doing it now is a breaking change to
   `TransportSink::ping` and `TransportEvent::Pong`, which ripples into
   `ocpp-transport-embassy-net` and `tests/common`. Doing it later means a second breaking change
   after consumers have adopted keepalive. *Recommendation:* now, in the same release as the
   `ClientConfig` change, so there's one breaking release rather than two.
3. **Fixed interval vs idle-reset.** A fixed interval is the straightforward reading of
   `WebSocketPingInterval`. Resetting the timer on any inbound traffic saves pings on a busy link
   but is more machinery and arguably less spec-literal. *Recommendation:* fixed interval;
   revisit only if a real deployment complains.

## Sequencing

Phases 0 → 1 → 2 → 3 land in order (each depends on its predecessor). Phase 4 depends on 2 but
not 3, so it can ship as soon as the loop exists — worth doing, since the getter/setter is the
part the downstream consumer is actually blocked on. Phase 5 is independent of all of them and can
go first; it's the cheapest item here and prevents a repeat of this triage.

CI must stay green throughout — including the three `no_std` proof builds, since phases 1–3 all
touch `client.rs` and `transport.rs`, both of which are on the `no_std` path, and the `embedded`
job builds `ocpp-transport-embassy-net` against the real transport traits that phase 1 may change.
