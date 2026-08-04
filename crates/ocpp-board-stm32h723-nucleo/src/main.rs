//! NUCLEO-H723ZG firmware scaffold: brings up the board's onboard LAN8742A Ethernet PHY and
//! hardware RNG, connects to a CSMS over OCPP 1.6/WebSocket via `ocpp-transport-embassy-net`,
//! and sends a Heartbeat every 30s as a smoke test. See this crate's README for status,
//! what's been verified vs. not, and what to edit before pointing this at a real CSMS.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cell::RefCell;
use core::time::Duration as CoreDuration;

use defmt::{error, info, warn};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_net::{Ipv4Address, StackResources};
use embassy_stm32::eth::{Ethernet, GenericPhy, PacketQueue, Sma};
use embassy_stm32::peripherals::{ETH, ETH_SMA, RNG};
use embassy_stm32::rng::Rng;
use embassy_stm32::{Config as StmConfig, bind_interrupts, eth, peripherals, rng};
use embassy_time::Timer;
use embedded_alloc::LlffHeap as Heap;
use ocpp_client::ocpp_1_6::OCPP1_6Client;
use ocpp_client::rust_ocpp::v1_6::messages::heart_beat::HeartbeatRequest;
use ocpp_client::{Client, ReconnectPolicy};
use ocpp_transport_embassy_net::{
    ConnectConfig, EmbassyExecutor, EmbassyNetReconnector, EmbassyTimer, RngFactory,
};
use panic_probe as _;
use rand_core::RngCore;
use static_cell::StaticCell;

// ---------------------------------------------------------------------------------------------
// EDIT ME: your CSMS address, path, and OCPP identity. These are placeholders (same pattern
// embassy's own examples use for their demo server address) - this firmware will not connect to
// anything real until you change them.
// ---------------------------------------------------------------------------------------------
const CSMS_ADDR: Ipv4Address = Ipv4Address::new(192, 168, 1, 10);
const CSMS_PORT: u16 = 9000;
const CSMS_HOST: &str = "192.168.1.10";
const CHARGE_POINT_PATH: &str = "/ocpp/CP001";

/// Locally-administered placeholder MAC (bit 1 of the first byte set, per IEEE 802). Fine for
/// one board on a bench; give each real device a unique address (e.g. derived from the H723's
/// 96-bit unique ID register, `embassy_stm32::uid`) before deploying more than one.
const MAC_ADDR: [u8; 6] = [0x02, 0x00, 0x11, 0x22, 0x33, 0x44];

type EthDevice = Ethernet<'static, ETH, GenericPhy<Sma<'static, ETH_SMA>>>;

bind_interrupts!(struct Irqs {
    ETH => eth::InterruptHandler;
    RNG => rng::InterruptHandler<peripherals::RNG>;
});

#[global_allocator]
static HEAP: Heap = Heap::empty();

/// The hardware RNG, set once by `main()` and shared from then on: as the source behind
/// `RngFactory` (WebSocket frame masking, `ocpp_transport_embassy_net::ConnectConfig`) *and* as
/// `getrandom`'s custom backend (`uuid`'s `v4` feature, used for OCPP-J message IDs - see
/// `__getrandom_v03_custom` below and this crate's README for why bare-metal targets need one).
/// One hardware RNG peripheral, two consumers, shared via `critical_section` instead of
/// duplicated (it can't be duplicated - it's a singleton peripheral).
static RNG_CELL: critical_section::Mutex<RefCell<Option<Rng<'static, RNG>>>> =
    critical_section::Mutex::new(RefCell::new(None));

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, EthDevice>) -> ! {
    runner.run().await
}

/// Hands the hardware RNG out through `critical_section` so it can back
/// `ocpp_transport_embassy_net::RngFactory` (`Fn() -> Box<dyn RngCore + Send>`) without needing
/// to duplicate the underlying peripheral - there's only one.
struct SharedHwRng;

impl RngCore for SharedHwRng {
    fn next_u32(&mut self) -> u32 {
        critical_section::with(|cs| {
            RNG_CELL
                .borrow_ref_mut(cs)
                .as_mut()
                .expect("RNG_CELL initialized before any RngFactory closure can run")
                .next_u32()
        })
    }
    fn next_u64(&mut self) -> u64 {
        critical_section::with(|cs| {
            RNG_CELL
                .borrow_ref_mut(cs)
                .as_mut()
                .expect("RNG_CELL initialized before any RngFactory closure can run")
                .next_u64()
        })
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        critical_section::with(|cs| {
            RNG_CELL
                .borrow_ref_mut(cs)
                .as_mut()
                .expect("RNG_CELL initialized before any RngFactory closure can run")
                .fill_bytes(dest)
        })
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

/// `getrandom`'s "custom backend" hook (see this crate's `.cargo/config.toml`'s
/// `--cfg getrandom_backend="custom"` and README) - required because bare-metal
/// `thumbv7em-none-eabihf` has no OS-provided entropy source for `getrandom` to auto-detect.
/// `uuid`'s `v4` feature (via `ocpp-client`'s use of `Uuid::new_v4()` for OCPP-J message IDs)
/// goes through this. Reuses the same hardware RNG as `SharedHwRng` above.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    critical_section::with(|cs| {
        let mut slot = RNG_CELL.borrow_ref_mut(cs);
        match slot.as_mut() {
            Some(rng) => {
                // SAFETY: `dest`/`len` come straight from getrandom's own `fill_inner`, which
                // guarantees a valid, writable buffer of `len` bytes.
                let buf = unsafe { core::slice::from_raw_parts_mut(dest, len) };
                rng.fill_bytes(buf);
                Ok(())
            }
            // Only reachable if something calls into getrandom before main() has run far enough
            // to initialize RNG_CELL - shouldn't happen in this firmware's own code paths, but
            // returning an error is safer than a bare panic if it ever does.
            None => Err(getrandom::Error::new_custom(1)),
        }
    })
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    // 64 KiB heap - comfortably inside NUCLEO-H723ZG's 320 KiB AXI SRAM alongside stacks and
    // static buffers. `ocpp-client` needs `alloc` unconditionally (BTreeMap/VecDeque/Arc
    // bookkeeping); bump this if you see allocation failures once you add more of your own code.
    {
        const HEAP_SIZE: usize = 64 * 1024;
        static mut HEAP_MEM: [core::mem::MaybeUninit<u8>; HEAP_SIZE] =
            [core::mem::MaybeUninit::uninit(); HEAP_SIZE];
        unsafe {
            #[allow(static_mut_refs)]
            HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE);
        }
    }

    // Clock tree: HSI -> PLL1 -> 400 MHz sysclk, 200 MHz AHB, 100 MHz APB1-4, VOS Scale1. Copied
    // from embassy's own examples/stm32h7/src/bin/eth.rs (github.com/embassy-rs/embassy) rather
    // than hand-derived - it's a proven-working config for this MCU family, including the HSI48
    // enable RNG needs. Push toward H723's ~550 MHz ceiling later if you need it; this is the
    // conservative starting point.
    let mut config = StmConfig::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hsi = Some(HSIPrescaler::DIV1);
        config.rcc.csi = true;
        config.rcc.hsi48 = Some(Default::default()); // needed for RNG
        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSI,
            prediv: PllPreDiv::DIV4,
            mul: PllMul::MUL50,
            divp: Some(PllDiv::DIV2),
            divq: None,
            divr: None,
        });
        config.rcc.sys = Sysclk::PLL1_P; // 400 MHz
        config.rcc.ahb_pre = AHBPrescaler::DIV2; // 200 MHz
        config.rcc.apb1_pre = APBPrescaler::DIV2; // 100 MHz
        config.rcc.apb2_pre = APBPrescaler::DIV2; // 100 MHz
        config.rcc.apb3_pre = APBPrescaler::DIV2; // 100 MHz
        config.rcc.apb4_pre = APBPrescaler::DIV2; // 100 MHz
        config.rcc.voltage_scale = VoltageScale::Scale1;
    }
    let p = embassy_stm32::init(config);
    info!("clocks up");

    // Hardware RNG, shared behind RNG_CELL (module-level `critical_section::Mutex`) so both
    // ConnectConfig's RngFactory and getrandom's custom backend (`__getrandom_v03_custom`
    // above) can use the one singleton peripheral without duplicating it.
    critical_section::with(|cs| {
        RNG_CELL.borrow_ref_mut(cs).replace(Rng::new(p.RNG, Irqs));
    });
    let rng_factory: RngFactory = Arc::new(|| Box::new(SharedHwRng) as Box<dyn RngCore + Send>);

    // Random seed for embassy-net's own internal use (TCP ISN randomization etc.), independent
    // of the RngFactory above (which is specifically for embedded-websocket's frame masking).
    let mut seed_bytes = [0u8; 8];
    rng_factory().fill_bytes(&mut seed_bytes);
    let net_seed = u64::from_le_bytes(seed_bytes);

    // Onboard LAN8742A PHY over RMII. Pin mapping is specific to NUCLEO-H723ZG (cross-checked
    // against stm32-rs/stm32h7xx-hal's own ethernet-rtic-nucleo-h723zg.rs example) - if you're
    // on different H723 hardware, these will very likely be wrong for you; check your board's
    // schematic.
    static PACKETS: StaticCell<PacketQueue<4, 4>> = StaticCell::new();
    let device: EthDevice = Ethernet::new(
        PACKETS.init(PacketQueue::new()),
        p.ETH,
        Irqs,
        p.PA1,  // REF_CLK
        p.PA7,  // CRS_DV
        p.PC4,  // RXD0
        p.PC5,  // RXD1
        p.PG13, // TXD0
        p.PB13, // TXD1
        p.PG11, // TX_EN
        MAC_ADDR,
        p.ETH_SMA,
        p.PA2, // MDIO
        p.PC1, // MDC
    );
    info!("ethernet driver up");

    let net_config = embassy_net::Config::dhcpv4(Default::default());
    static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(
        device,
        net_config,
        RESOURCES.init(StackResources::new()),
        net_seed,
    );
    spawner.spawn(net_task(runner).unwrap());

    info!("waiting for DHCP...");
    stack.wait_config_up().await;
    info!("network up: {}", stack.config_v4().unwrap().address);

    let connect_config = ConnectConfig::new(
        stack,
        (CSMS_ADDR, CSMS_PORT).into(),
        CSMS_HOST,
        CHARGE_POINT_PATH,
        "ocpp1.6",
        rng_factory,
    );

    // Initial connect: Client::from_transport_with_reconnect needs an already-connected
    // transport up front (the Reconnector only kicks in for *later* redials), so retry here
    // ourselves until the first connection succeeds.
    let (sink, stream) = loop {
        match ocpp_transport_embassy_net::connect(&connect_config).await {
            Ok(pair) => break pair,
            Err(err) => {
                error!("initial connect failed: {}", defmt::Debug2Format(&err));
                Timer::after_secs(5).await;
            }
        }
    };
    info!("connected to CSMS");

    let client: OCPP1_6Client = Client::from_transport_with_reconnect(
        sink,
        stream,
        CoreDuration::from_secs(30),
        Box::new(EmbassyExecutor::new(spawner)),
        Box::new(EmbassyTimer),
        Some(Box::new(EmbassyNetReconnector::new(connect_config))),
        ReconnectPolicy::default(),
    );

    loop {
        match client.send_heartbeat(HeartbeatRequest {}).await {
            Ok(response) => info!(
                "heartbeat ok, CSMS clock: {}",
                defmt::Debug2Format(&response.current_time)
            ),
            Err(err) => warn!("heartbeat failed: {}", defmt::Debug2Format(&err)),
        }
        Timer::after_secs(30).await;
    }
}
