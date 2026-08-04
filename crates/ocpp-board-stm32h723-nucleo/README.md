# ocpp-board-stm32h723-nucleo

Firmware scaffold for the **NUCLEO-H723ZG** dev board: brings up its onboard LAN8742A Ethernet
PHY (RMII) and hardware RNG, gets a DHCP address, connects to a CSMS over OCPP 1.6/WebSocket via
[`ocpp-transport-embassy-net`](../ocpp-transport-embassy-net), and sends a `Heartbeat` every 30s
as a smoke test. Not a library - a runnable example firmware for this one specific board.

## Status: builds and links for the real target, not yet run on real hardware

`cargo build --target thumbv7em-none-eabihf` (see "Building" below) produces a real, linked ARM
firmware binary - the full toolchain (compile, link, `--cfg getrandom_backend="custom"` resolved,
`memory.x` auto-generated) works end to end. **It has not been flashed to a real board or tested
against a real CSMS yet.** The clock tree and Ethernet pin mapping are taken from proven-working
reference sources (below), not guessed, but "compiles and links" and "runs correctly on hardware"
are different bars - this is the former, not yet the latter.

## What's here

- Clock tree: HSI → PLL1 → 400 MHz sysclk / 200 MHz AHB / 100 MHz APB1-4, VOS Scale1, HSI48
  enabled (required for the RNG peripheral). Copied from
  [`embassy-rs/embassy`'s own `examples/stm32h7/src/bin/eth.rs`](https://github.com/embassy-rs/embassy/blob/main/examples/stm32h7/src/bin/eth.rs) -
  a conservative, working config for this MCU family, not hand-derived. H723 can reach ~550 MHz
  with more aggressive tuning if you need it later.
- Ethernet: RMII pins `PA1` (REF_CLK), `PA7` (CRS_DV), `PC4`/`PC5` (RXD0/1), `PG13`/`PB13`
  (TXD0/1), `PG11` (TX_EN), `PA2`/`PC1` (MDIO/MDC). Specific to **NUCLEO-H723ZG** - cross-checked
  against two independent sources agreeing exactly:
  [`stm32-rs/stm32h7xx-hal`'s `ethernet-rtic-nucleo-h723zg.rs` example](https://github.com/stm32-rs/stm32h7xx-hal/blob/master/examples/ethernet-rtic-nucleo-h723zg.rs)
  (written for this exact board) and embassy's generic stm32h7 `eth.rs` example above (whose
  comment flags that some H7 variants use a different TXD1 pin - PG12 instead of PB13 - and
  NUCLEO-H723ZG uses PB13, matching the HAL example). If you're on different H723 hardware,
  these pins are very likely wrong for you - check your schematic.
- Hardware RNG (`embassy_stm32::rng::Rng`), shared via a `critical_section`-guarded `RefCell`
  (`RNG_CELL`) between two consumers: `ocpp_transport_embassy_net::RngFactory` (WebSocket frame
  masking) and `getrandom`'s custom backend (`uuid`'s `v4` feature, used for OCPP-J message IDs -
  see "The getrandom problem" below). One physical peripheral, can't be duplicated, so both paths
  go through the same cell instead of trying to own it twice.
- `embedded-alloc`'s `LlffHeap`, 64 KiB, comfortably inside the H723's 320 KiB AXI SRAM.
- DHCP via `embassy-net`; `ConnectConfig`/`connect()`/`EmbassyNetReconnector` from
  `ocpp-transport-embassy-net` wired up once DHCP completes.

## Two things you must edit before this connects to anything real

1. **`CSMS_ADDR`/`CSMS_PORT`/`CSMS_HOST`/`CHARGE_POINT_PATH`** at the top of `src/main.rs` are
   placeholders (`192.168.1.10:9000`, same pattern embassy's own examples use for their demo
   server address) - this firmware won't reach a real CSMS until you point it at yours.
2. **`MAC_ADDR`** is a fixed locally-administered placeholder. Fine for one board on a bench; if
   you deploy more than one, give each a unique address (e.g. derived from the H723's 96-bit
   unique ID register, `embassy_stm32::uid`) or you'll get MAC collisions on the network.

## The `getrandom` problem (and how this crate solves it)

`ocpp-client` uses `uuid`'s `v4` feature for OCPP-J message IDs, which pulls in `getrandom`.
`getrandom` auto-detects a backend per `target_os` - bare-metal `thumbv7em-none-eabihf` has none,
so without intervention the build fails with `compile_error!("target is not supported...")`. This
firmware's `.cargo/config.toml` sets `--cfg getrandom_backend="custom"`, and `src/main.rs` defines
the actual backend function (`__getrandom_v03_custom`), backed by the same hardware RNG used for
WebSocket frame masking. See
[`ocpp-transport-embassy-net`'s README](../ocpp-transport-embassy-net/README.md) for the general
explanation - this crate is the concrete "here's a firmware binary that actually does it" half.

## Building

```sh
rustup target add thumbv7em-none-eabihf
RUSTFLAGS='--cfg getrandom_backend="custom"' cargo build -p ocpp-board-stm32h723-nucleo --target thumbv7em-none-eabihf
RUSTFLAGS='--cfg getrandom_backend="custom"' cargo clippy -p ocpp-board-stm32h723-nucleo --target thumbv7em-none-eabihf -- -D warnings
```

Both are also run in CI (`.github/workflows/ci.yaml`'s `embedded` job) on every push/PR.

Note this crate is **not** in the workspace's `default-members` (see the root `Cargo.toml`'s
comment) - a plain `cargo build`/`test` at the repo root won't try to build it, since `cortex-m`'s
inline `asm!` only compiles for a real ARM target and would otherwise fail outright on your host.
Always pass `-p ocpp-board-stm32h723-nucleo --target thumbv7em-none-eabihf` explicitly.

## Flashing / running

`.cargo/config.toml` configures [`probe-rs`](https://probe.rs/) as the runner (works with the
board's onboard ST-LINK debugger over USB):

```sh
cargo install probe-rs-tools --locked
RUSTFLAGS='--cfg getrandom_backend="custom"' cargo run -p ocpp-board-stm32h723-nucleo --target thumbv7em-none-eabihf
```

`defmt` log output (`DEFMT_LOG=info` by default, set in `.cargo/config.toml`) streams back over
RTT. Swap the `runner` line for an OpenOCD invocation if you'd rather not use probe-rs.

## Next steps

1. Flash it, point it at a real CSMS (or a local OCPP simulator), and see what breaks - the
   buffer/fragmentation handling in `ocpp-transport-embassy-net` is the part most likely to need
   iteration once it meets real network conditions.
2. TLS (`wss://`) - see `ocpp-transport-embassy-net`'s README.
3. Derive a per-device MAC (and OCPP charge point identity) from the H723's unique ID register
   instead of the fixed placeholder, once you're past a single bench board.
