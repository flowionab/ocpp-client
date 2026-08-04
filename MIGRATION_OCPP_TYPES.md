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
