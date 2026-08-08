//! Guards against the gap that produced a downstream bug report: an OCPP action whose
//! request/response types exist in `ocpp-types` but which no `ocpp_*_action!` invocation ever
//! wired up, so callers had no way to send or receive it.
//!
//! Five such actions shipped that way and were only noticed when a consumer tried to use them
//! (see CHANGELOG 0.2.1). Nothing about that was visible at compile time - unwired types are
//! simply unreferenced - so this test makes it a build failure instead of a bug report.
//!
//! It works by comparing action *name strings*, not type names: every action type in
//! `ocpp-types` carries the wire name as `<Type as ocpp_types::Action>::ACTION`, and every macro
//! invocation here passes the same string. Rust has no reflection to enumerate types with, so the
//! `ocpp-types` side is read from its source, located through `cargo metadata` (the crate is laid
//! out one file per message type, with a single `const ACTION` in each).

use std::path::PathBuf;
use std::process::Command;

/// Where cargo unpacked the exact `ocpp-types` this crate compiles against.
fn ocpp_types_src() -> PathBuf {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1"])
        .arg("--manifest-path")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .output()
        .expect("cargo metadata should run inside a cargo test");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata emits JSON");

    // Match on the package entry, not on the first mention of the name - dependency records carry
    // the name too, and their enclosing object's manifest_path is the *dependent's*.
    let manifest = metadata["packages"]
        .as_array()
        .expect("metadata has a packages array")
        .iter()
        .find(|package| package["name"] == "ocpp-types")
        .and_then(|package| package["manifest_path"].as_str())
        .map(PathBuf::from)
        .expect("ocpp-types should be a dependency of this crate");

    manifest
        .parent()
        .expect("manifest has a parent directory")
        .join("src")
}

/// Every action name `ocpp-types` defines for `version`, read from the `const ACTION` in each
/// `*_request.rs`. Request files only: a response carries the same name, and counting both would
/// just duplicate.
fn actions_in_ocpp_types(version: &str) -> Vec<String> {
    let dir = ocpp_types_src().join(version);
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("could not read {}: {e}", dir.display()));

    let mut actions = Vec::new();
    for entry in entries {
        let path = entry.expect("readable dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with("_request.rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("readable source file");
        if let Some(action) = extract_after(&source, "const ACTION: &'static str = \"") {
            actions.push(action);
        }
    }
    assert!(
        !actions.is_empty(),
        "found no actions in {} - has ocpp-types' layout changed?",
        dir.display()
    );
    actions.sort();
    actions
}

/// Every action name wired up in this crate's `actions.rs` for one version, taken from the
/// name-string argument of each macro invocation.
fn actions_wired_up(module: &str, macro_name: &str) -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(module)
        .join("actions.rs");
    let source = std::fs::read_to_string(&path).expect("readable actions.rs");

    let mut actions = Vec::new();
    for invocation in source.split(&format!("{macro_name}!")).skip(1) {
        // The name string is the first quoted literal in the invocation - the arguments before it
        // are all bare identifiers (marker type, request type, response type).
        let Some(open) = invocation.find('"') else {
            continue;
        };
        let rest = &invocation[open + 1..];
        let Some(close) = rest.find('"') else {
            continue;
        };
        actions.push(rest[..close].to_string());
    }
    actions.sort();
    actions
}

fn extract_after(source: &str, key: &str) -> Option<String> {
    let start = source.find(key)? + key.len();
    let rest = &source[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// `ocpp_*_action!` models a CALL/CALLRESULT pair. Actions that are `SEND`-only (OCPP-J 2.1
/// message type 6, no response) go through `ocpp_*_send_action!` instead, and their `ocpp-types`
/// definition has no `*_request.rs` file - so they are wired up without appearing on the
/// `ocpp-types` side of this comparison.
fn assert_every_action_is_wired(version: &str, module: &str, macro_prefix: &str) {
    let available = actions_in_ocpp_types(version);
    let mut wired = actions_wired_up(module, &format!("{macro_prefix}_action"));
    wired.extend(actions_wired_up(
        module,
        &format!("{macro_prefix}_send_action"),
    ));

    let missing: Vec<_> = available
        .iter()
        .filter(|action| !wired.contains(action))
        .collect();

    assert!(
        missing.is_empty(),
        "{version}: ocpp-types defines these actions but no {macro_prefix}_action! invocation in \
         src/{module}/actions.rs wires them up, so callers cannot send or receive them: \
         {missing:?}\n\
         Add one macro line each (see CLAUDE.md, \"Adding a new OCPP action\").",
    );
}

#[cfg(feature = "ocpp_1_6")]
#[test]
fn every_ocpp_1_6_action_in_ocpp_types_is_wired_up() {
    assert_every_action_is_wired("v16", "ocpp_1_6", "ocpp_1_6");
}

#[cfg(feature = "ocpp_2_0_1")]
#[test]
fn every_ocpp_2_0_1_action_in_ocpp_types_is_wired_up() {
    assert_every_action_is_wired("v201", "ocpp_2_0_1", "ocpp_2_0_1");
}

#[cfg(feature = "ocpp_2_1")]
#[test]
fn every_ocpp_2_1_action_in_ocpp_types_is_wired_up() {
    assert_every_action_is_wired("v21", "ocpp_2_1", "ocpp_2_1");
}

/// Sanity check on the scan itself: if either side silently stopped matching anything, the
/// coverage tests above would pass vacuously.
#[cfg(feature = "ocpp_2_1")]
#[test]
fn the_coverage_scan_actually_finds_actions_on_both_sides() {
    let available = actions_in_ocpp_types("v21");
    let wired = actions_wired_up("ocpp_2_1", "ocpp_2_1_action");

    assert!(
        available.len() > 80,
        "expected ~90 actions in ocpp-types v21, found {}",
        available.len()
    );
    assert!(
        wired.len() > 80,
        "expected ~90 wired actions in src/ocpp_2_1/actions.rs, found {}",
        wired.len()
    );
    assert!(
        available.contains(&"GetDERControl".to_string()),
        "the scan should see GetDERControl - one of the five actions this test exists because of"
    );
}
