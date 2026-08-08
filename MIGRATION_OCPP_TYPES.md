# Migrating from `rust-ocpp` to `ocpp-types`

Roadmap for replacing the `rust-ocpp` fork with `ocpp-types` (crates.io, `flowionab` org,
currently `0.1.0`) as the source of OCPP 1.6J/2.0.1/2.1 message types. Background/rationale is
in the conversation that produced this doc; the short version:

- `ocpp-types` is `no_std`, allocation-free by default (`heapless`-backed bounded types
  generated from the official schemas), with an `alloc` feature that turns spec-unbounded
  fields into plain `alloc::String`/`Vec` - matching this crate's own std/no_std+alloc stance.
  Bounded fields (e.g. `IdTag`) stay `heapless::String<N>` regardless of the `alloc` feature,
  which is the main API-shape change call sites will feel (fallible `TryFrom` construction
  instead of a bare string).
- It covers 1.6/2.0.1/2.1 in full, including `NotifyReport` for 2.1 - closing the gap
  documented in `CLAUDE.md` and `PRODUCTION_READINESS.md` item 4 (blocked upstream on
  `rust-ocpp`'s `wip_v2_1::messages::notify_report` being an empty module).
- Its `serde` feature derives ordinary `serde::Serialize`/`Deserialize` (its
  `serde-json-core` helpers on `Action` are optional convenience methods, not a hard
  requirement), so `client.rs`'s existing `serde_json::Value`-based dynamic dispatch
  (`serde_json::from_value::<A::Request>(payload)`) keeps working unchanged - no rewrite of
  the engine's dispatch design needed.
- `ocpp-types` has no per-version cargo features (`v16`/`v201`/`v21` all compile in
  unconditionally); this crate's own `ocpp_1_6`/`ocpp_2_0_1`/`ocpp_2_1` features stop
  forwarding to `rust-ocpp/vX` and instead just gate this crate's own per-version modules.

## Steps

1. ~~**Add the dependency, standalone.**~~ **Done.** `ocpp-types = { version = "0.1.1",
   default-features = false, features = ["serde", "alloc"] }` alongside `rust-ocpp`. Confirmed
   both the default build and the `no_std`+`alloc` proof build resolve and compile with it
   present but unused.
2. ~~**Migrate 1.6.**~~ **Done.** `src/ocpp_1_6/actions.rs`'s imports now come from
   `ocpp_types::v16` (message structs) and `ocpp_types::v16::common` (nested field enums -
   these are *not* re-exported at the `v16` root, only under `common`, which
   `rust_ocpp::v1_6::types`'s flat namespace didn't require handling). `src/ocpp_1_6/error.rs`'s
   `define_error!` macro output was replaced with a small `OCPP1_6Error { code:
   ocpp_types::v16::RpcErrorCode, description, details }` struct implementing `ProtocolError` -
   no downstream code pattern-matched the old per-variant enum, so this was a safe shape change.
   `heapless::String::try_from`/`TryFrom` fallout was smaller than expected: none of the actions
   this crate's own tests/examples exercise happen to touch a bounded field directly (`IdTag`
   etc. show up in other actions not covered yet) - the actual fallout was renamed nested enum
   variants (`ResetRequest.kind: ResetRequestStatus` → `r#type: ResetRequestType`,
   `TriggerMessageStatus`/`MessageTrigger` → `TriggerMessageResponseStatus`/`RequestedMessage`)
   and `HeartbeatResponse.current_time` changing from `chrono::DateTime<Utc>` to a plain
   `String` (ocpp-types doesn't depend on chrono at all - timestamps are wire-format strings).
   Fixed in `tests/ocpp_1_6_{fake_transport,wait_for,websocket,reconnect,custom_tls}.rs` and
   `examples/connect.rs`. Full 1.6 test suite, clippy, fmt, and all three `no_std`+`alloc` proof
   builds are green with 2.0.1/2.1 still on `rust-ocpp`.
   - **Blocked and unblocked mid-step**: `ocpp-types` 0.1.0's 1.6 module had a real codegen bug,
     not just API-shape friction - its generator named shared enums after the JSON *property*
     name (`type`, `status`) rather than per-message, so e.g. `Reset.type` (`Hard`/`Soft`) and
     `ChangeAvailability.type` (`Inoperative`/`Operative`) collided into one `common::Type`,
     silently keeping only one message's variants. A full schema audit found this hit 3
     properties in 1.6 (`type`, `status`, `unit`) and as many as 14 distinct response messages
     via the `status` collision alone; 2.0.1/2.1 were unaffected (their schemas use uniquely-
     named `$ref` definitions, not anonymous inline enums, so the same bug class can't occur
     there). Fixed upstream in `ocpp-types` 0.1.1 (commit `2d8b411`, "Fix 1.6j inline-enum type
     collisions") - enums are now message-qualified (`ResetRequestType`,
     `ChangeAvailabilityRequestType`, etc.). Verified the fix by re-running the same audit
     script against every 1.6 schema: zero mismatches, zero unmatched value-sets. Bumped the
     dependency to `0.1.1` before continuing.
3. ~~**Migrate 2.0.1**~~ **Done.** Same pattern as 1.6: `src/ocpp_2_0_1/actions.rs` imports now
   come from `ocpp_types::v201`; `error.rs`'s `define_error!` output replaced with a
   `OCPP2_0_1Error { code: ocpp_types::v201::RpcErrorCode, description, details }` struct
   (variant set confirmed identical to the old hand-rolled list). No enum-collision bug here -
   2.0.1's schemas use named `$ref` definitions, not anonymous inline enums, so this went
   smoothly. Fallout in `tests/ocpp_2_0_1_{fake_transport,websocket}.rs`: field renames
   (`ResetRequest.request_type` → `r#type`, `ResetEnumType`/`ResetStatusEnumType` →
   `ResetEnum`/`ResetStatusEnum` under `ocpp_types::v201::common`, generator drops the
   schema's redundant `...Type` suffix), every message now carries an explicit
   `custom_data: Option<CustomData>` field 2.0.1's schemas define on every message but
   `rust-ocpp` apparently didn't surface the same way (`HeartbeatRequest {}` →
   `HeartbeatRequest { custom_data: None }`), and the same `current_time: String` (not
   `chrono::DateTime<Utc>`) change as 1.6. Full test suite, clippy, fmt, and all three
   `no_std`+`alloc` proof builds green with 2.1 still on `rust-ocpp`.
4. ~~**Migrate 2.1**~~ **Done.** Same pattern as 2.0.1: `src/ocpp_2_1/actions.rs` imports now
   come from `ocpp_types::v21`; `error.rs`'s `OCPP2_1Error` rewritten around
   `ocpp_types::v21::RpcErrorCode` (variant set confirmed identical to 2.0.1's, matching the
   crate's own comment that OCPP-J's RPC framework error codes didn't change between 2.0.1 and
   2.1). Fallout in `tests/ocpp_2_1_{fake_transport,websocket}.rs` was the same shape as
   2.0.1's: `ResetRequest.reset_type` → `r#type`, `ResetEnumType`/`ResetStatusEnumType` →
   `ResetEnum`/`ResetStatusEnum`, `current_time` from `chrono::DateTime<Utc>` to `String` - plus
   one new one: `rust-ocpp`'s 2.1 `ResetResponse.status` serialized lowercase (`"accepted"`,
   presumably a fork quirk), `ocpp_types` serializes the variant name as-is (`"Accepted"`) like
   every other version: test assertion updated to match the correct wire format.
   - **`NotifyReport` wired up for real**, closing `PRODUCTION_READINESS.md` item 4 - added the
     `ocpp_2_1_action!(NotifyReport, ...)` line and a new fake-transport test
     (`call_resolves_notify_report_now_that_its_wired_up`) proving a full CALL/CALLRESULT round
     trip, not just that the type compiles.
   - **One action needed a modeling compromise**: `NotifyPeriodicEventStream` is genuinely
     SEND-only in the OCPP-J 2.1 spec (no CALLRESULT reply), so `ocpp_types` models it as a
     single struct rather than a Request/Response pair - the only action in 2.1 shaped that way
     (confirmed via a full request/response file-listing diff across `ocpp-types`' `v21`
     module). `Client`'s engine only understands CALL/CALLRESULT/CALLERROR, not OCPP 2.1's
     additional SEND/CALLRESULTERROR frame types, so this crate was *already* modeling it as a
     call/response pair under `rust-ocpp` (with a synthetic Response type) before this
     migration - kept that same (spec-loose but pre-existing) behavior by using
     `NotifyPeriodicEventStream` as both the request and response type, renaming the generated
     marker type to `NotifyPeriodicEventStreamAction` to avoid colliding with the imported
     message type of the same name. Documented inline in `actions.rs`; real SEND-frame support
     would be a `Client` engine change, out of scope here.
   - Full test suite, clippy, fmt, and all three `no_std`+`alloc` proof builds green. Only
     `src/lib.rs`'s `pub use rust_ocpp;` re-export and
     `crates/ocpp-board-stm32h723-nucleo/src/main.rs` still reference `rust_ocpp` - exactly
     steps 5/6's scope.
5. ~~**Drop `rust-ocpp`.**~~ **Done.** Removed the dependency from `Cargo.toml` entirely -
   `Cargo.lock` no longer mentions it, and it's no longer even downloaded on a clean build.
   `src/lib.rs`'s `pub use rust_ocpp;` became `pub use ocpp_types;` (same rationale as the
   existing `pub use rustls;` re-export: callers can name request/response types using the
   exact version this crate was compiled against, without pinning a matching version
   themselves). The `ocpp_1_6`/`ocpp_2_0_1`/`ocpp_2_1` Cargo features no longer forward to
   anything (`ocpp-types` has no per-version features to forward to) - they now just gate this
   crate's own `src/ocpp_{1_6,2_0_1,2_1}/` modules, and `std` no longer forwards to
   `rust-ocpp/std` (nothing left to forward to - `ocpp-types`'s `alloc` feature is enabled
   unconditionally in `[dependencies]` regardless of this crate's own `std` status, matching
   the crate's existing no_std+alloc-only stance).
6. ~~**Update the embedded satellite crates**~~ **Done.**
   `crates/ocpp-board-stm32h723-nucleo/src/main.rs` was the one confirmed direct `rust_ocpp`
   consumer outside `ocpp-client` itself - swapped its `ocpp_client::rust_ocpp::v1_6::messages::
   heart_beat::HeartbeatRequest` import for `ocpp_client::ocpp_types::v16::HeartbeatRequest`
   (construction site, `HeartbeatRequest {}`, needed no change - same empty-struct shape).
   Verified with the real commands from that crate's README:
   `RUSTFLAGS='--cfg getrandom_backend="custom"' cargo build/clippy -p
   ocpp-board-stm32h723-nucleo --target thumbv7em-none-eabihf -- -D warnings` - both clean
   (one pre-existing, unrelated linker alignment warning on `.text`, not introduced by this
   change). `crates/ocpp-transport-embassy-net` doesn't touch message types (transport-layer
   only) - confirmed unaffected, zero `rust_ocpp` references before or after.
7. ~~**Re-verify the full CI matrix**~~ **Done.** Ran every job's exact command locally: `fmt`,
   `clippy --all-targets --all-features -- -D warnings`, `test` (`cargo build` +
   `cargo test --features test`), the three `no_std` proof builds, and `embedded`
   (`-p ocpp-transport-embassy-net` check+clippy, `-p ocpp-board-stm32h723-nucleo` full-link
   build+clippy, both against `thumbv7em-none-eabihf` with `RUSTFLAGS: --cfg
   getrandom_backend="custom"`). All green (one pre-existing, unrelated linker alignment
   warning on the board firmware's `.text` section - not a clippy/build failure, not introduced
   by this migration).
8. ~~**Update docs**~~ **Done.** `CLAUDE.md`'s `rust-ocpp` fork section rewritten for
   `ocpp-types` (status bullet, the `no_std`+`alloc` bullet's `rust-ocpp` mention, and the
   "Adding a new OCPP action" sections' request/response type references); `README.md`'s
   protocol table (`NotifyReport` caveat removed - it's implemented now), architecture diagram,
   and "Flowion OCPP Ecosystem" section (`rust-ocpp` → `ocpp-types` throughout);
   `PRODUCTION_READINESS.md` item 4 marked done (references this file + the new
   `NotifyReport` test) and item 5 updated - the git-fork blocker is gone (`ocpp-types` is a
   real crates.io release), though `ocpp-types` itself being early `0.1.x` is now the thing to
   pin deliberately rather than assume stable.

## Risk

`ocpp-types` is early (`0.1.x`) and from the same org as the `rust-ocpp` fork - the 1.6
enum-collision bug hit during step 2 (see above) confirms that's a real, not theoretical, risk:
it's a generated crate, so bugs are systemic across every message a given codegen mistake
touches rather than isolated. On the other hand, the turnaround was fast (reported and fixed
same-day, 0.1.0 → 0.1.1) since this crate is maintained by the same org this repo already
depends on for `rust-ocpp`. Worth re-running the same kind of schema-vs-generated-code audit
(see step 2's collision-detection script, not checked into this repo) before trusting 2.0.1/2.1
blindly in step 3-4, even though the structural reason for the 1.6 bug (anonymous inline enums)
doesn't apply to their `$ref`-based schemas.

---

# `ocpp-types` 0.1.3 → 0.2.0 (this crate's 0.4.0)

A second migration, on the same dependency. Four upstream changes, none of which touched the
engine - `client.rs`, `envelope.rs`, `transport.rs`, `error.rs`, `runtime.rs` and `sync.rs` all
compiled against 0.2.0 with no edits at all, which is the same result 2.0.1 and 2.1 gave when they
were ported.

1. **`dateTime` fields became `OcppTimestamp`.** Upstream's reason is size: not one of the 142
   `dateTime` fields across the three versions declares a `maxLength`, so as unbounded strings
   they each reserved the generator's 1024-byte default (`v16::HeartbeatResponse` was 1,032 bytes
   to carry a 24-character instant). The type is 16 bytes, allocator-free and comparable.

   For this crate the consequence is behavioural, not just structural: timestamps are *parsed*
   now, so a `dateTime` the CSMS writes badly fails the call as `ClientError::Decode` rather than
   reaching the caller as a bad string, and two timestamps naming the same instant in different
   UTC offsets compare equal. Fractional seconds (any precision) and non-UTC offsets survive a
   round trip. All of it is pinned by `tests/ocpp_1_6_timestamps.rs`. Everything else was
   mechanical: ~30 sites in `tests/`, `benches/` and `examples/` moved from string literals to
   `OcppTimestamp::parse_rfc3339(..)`.

2. **2.0.1/2.1 message types gained a `customData` type parameter**, defaulting upstream to the
   new zero-sized `NoCustomData` (which accepts a `customData` object and discards it). Taking
   that default unchanged would have silently downgraded every 2.x caller's `custom_data`, so the
   action markers this crate generates carry the parameter themselves -
   `pub struct Reset<C = CustomData>(PhantomData<fn() -> C>)` - defaulting to the specification's
   own shape. A deployment with a richer vendor extension names its own type
   (`client.call::<Reset<AcmeExtension>>(..)`), which was previously only possible by
   hand-writing an `Action` impl.

   The generated `send_*`/`on_*`/`wait_for_*` methods were deliberately **not** made generic. A
   generic method breaks inference at every call site that builds a request inline with
   `custom_data: None` - a defaulted type parameter does not participate in inference - which
   would have put a turbofish on the common path to serve the rare one. The same inference rule
   is why a handful of test-side literals now need `let request: ResetRequest = ..`.

   Two mechanical consequences inside the macros: `$req`/`$res` had to become `ident` fragments,
   since a `ty` fragment cannot be followed by generic arguments; and the marker holds
   `PhantomData<fn() -> C>` rather than `PhantomData<C>` so it stays `Send + Sync` - which
   `Action` requires of the marker itself - whatever `C` is.

3. **Eleven new OCPP 1.6 security-whitepaper actions.** `tests/action_coverage.rs` failed the
   moment the dependency was bumped, listing exactly those eleven, which is what that test was
   built for. One `ocpp_1_6_action!` line each, plus `tests/ocpp_1_6_security_actions.rs`
   covering a round trip apiece (`send_*` for the four a charge point initiates, `on_*` for the
   seven a CSMS initiates). 1.6 went 28 → 39; 2.0.1 and 2.1 were unchanged.

4. **Additive extras.** `alloc` is now upstream's default feature (this crate names its features
   explicitly, so nothing changed); a new `chrono` feature adds `OcppTimestamp` ↔
   `chrono::DateTime` conversions, forwarded through this crate's own `chrono` feature; and each
   version gained a `standard` module of value-set enums for fields the schemas type as bare
   strings (`SecurityEventNotification.type`, `Variable.name`, 1.6's configuration keys). The
   `standard` modules need no wiring - they are reachable as
   `ocpp_client::ocpp_types::v16::standard::*`.

Two smaller shifts show up when reading old code. `v16::common::RequestedMessage` is now
`TriggerMessageRequestRequestedMessage`, to make room for the `ExtendedTriggerMessage` variant of
the same idea (variants unchanged). And 48 2.x fields whose length the specification delegates to
a configuration variable - certificates, chains, CSRs, OCSP results, `MessageContent.content` -
moved from `heapless::String<N>` to `String`, so their construction drops from
`try_into().unwrap()` to `into()`. Only one site in this repo was affected, and clippy's
`unnecessary_fallible_conversions` found it; the compiler catches the reverse direction.

Upstream's remaining `0.2.0` headline - per-field const-generic capacities, and the sizing table
in its crate docs - does not reach consumers of this crate: those parameters only exist in
`ocpp-types`' allocation-free shape, and this crate enables `alloc` unconditionally. If that ever
changes, every message type acquires a second family of parameters and the macros will need the
same treatment `customData` just got.

---

# `ocpp-types` 0.2.0 → 0.3.0 (this crate's 0.5.0)

A non-event, and worth recording as one. Upstream 0.3.0 is **purely additive**: a `diff -ru` of
the two `src/` trees has no removed line anywhere, the action counts are identical (39 / 64 / 91),
and no message type, field type or enum variant changed shape. `cargo build` and the full
`cargo test --features test,chrono` suite both passed against it with **zero edits to `src/`,
`tests/`, `benches/` or `examples/`** - the first time a bump on this dependency has cost nothing
at all. The only source change in this repo is the version string in `Cargo.toml`.

The one addition is an opt-in **`validate` feature**: a `Validate` trait implemented for every
message type, covering the spec constraints the type system cannot carry. Those are the
`maxLength`s on fields too large to inline as a `heapless::String` (which are a plain `String`
under `alloc` - exactly the 48 fields 0.2.0 relaxed, plus the rest), and every `minItems`,
`minimum`, `maximum` and `multipleOf` in the schemas, none of which a collection or integer type
expresses at all. Errors carry the JSON path to the failing value (`ValidationError::in_field`
builds it up as the check unwinds), so a rejection names the field.

**This crate forwards it, off by default, and adds the one piece upstream deliberately left to
this layer.** `ValidationErrorKind::constraint_class` classifies a violation as property- or
occurrence-class, but stops there: upstream's doc says the caller picks the code "from their
version's `RpcErrorCode`", and there are three of those. They disagree by one letter - 1.6J's
table really does read `OccurenceConstraintViolation`, which 2.0.1 and 2.1 corrected to
`Occurrence`. This crate owns all three error enums, so `From<ValidationError>` for
`OCPP1_6Error`/`OCPP2_0_1Error`/`OCPP2_1Error` is the natural home for that mapping; every
consumer would otherwise rediscover the typo. The impls also fill `errorDetails` with the JSON
path alone (`{"path": "id[0]"}`), separately from the sentence in `description`, so a peer can
match on it mechanically. `tests/validation_error_mapping.rs` covers all three versions,
including a fake-transport case proving a handler's `request.validate()?` reaches the wire as the
right code.

**What was *not* done is validate automatically inside `Client::call`.** The blocker is
structural rather than a matter of taste: `A::Request` would need a `Validate` bound, and putting
one behind `#[cfg(feature = "validate")]` makes the `Action` trait change shape with a feature
flag. Cargo features unify across the whole dependency graph, so one crate enabling `validate`
would break an unrelated crate's custom `Action` impl - and `src/action.rs` documents that trait
as open to consumers, so the cost lands squarely on the extension path. Two supporting reasons:
`ClientError` would grow from 64 to ~304 bytes (a `ValidationError` is 296, holding its path
inline), and the outbound payoff is thin, since a station builds messages from bounded fields the
types already make unrepresentable. Callers write `request.validate()?` where they want it. The
full argument is PRODUCTION_READINESS.md item 5.

One incidental finding worth keeping: **no 1.6 schema states `minItems` at all**, so no generated
1.6 message can produce an occurrence-class violation. The 1.6 mapping still has to be right -
a consumer validating a custom `Action` payload can raise one - which is why that test builds the
error by hand rather than from a payload.
