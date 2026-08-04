//! No hand-written `memory.x` here - `embassy-stm32`'s `memory-x` Cargo feature (enabled in
//! this crate's Cargo.toml) generates a correct one for `stm32h723zg` from its own chip
//! database and adds it to the linker search path via its own build script's
//! `cargo:rustc-link-search` (which Cargo propagates into this binary's final link step
//! automatically). This build script only adds the linker flags `cortex-m-rt`/`defmt-rtt`
//! expect - `--nmagic`, `-Tlink.x` (from `cortex-m-rt`), `-Tdefmt.x` (from `defmt-rtt`) -
//! mirroring embassy's own `examples/stm32h7/build.rs`.

fn main() {
    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
}
