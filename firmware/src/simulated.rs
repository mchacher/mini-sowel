//! The engine on the simulated radio, for the image QEMU runs (feature `sim`, spec 016): the
//! realistic home of `fixtures/` is written to `/data` when it holds no `devices.json` (`seed`),
//! then the engine plays the fixture scenario, in its own thread, as `--sim --serve` does on the Mac but
//! without HTTP. When the scenario has played its last event, the loop stops, the saver writes,
//! and the memory of the run is measured.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use esp_idf_svc::sys::esp_fill_random;
use solite_core::EngineStats;
use solite_core::devices::lock;
use solite_core::engine::{self, Clocks, DEVICES_FILE, Saver, Storage, load_home};
use solite_core::radio::{Radio, RadioTransport, TransportError};
use solite_sim::{RealClock, Scenario, SimTransport};

use crate::measure::{self, Lows};
use crate::memory;
use crate::storage::{BASE, FatStorage, write_text};

/// The files of the realistic home, written to a blank `/data`.
const SEED: [(&str, &str); 5] = [
    ("devices.json", include_str!("../fixtures/devices.json")),
    ("zones.json", include_str!("../fixtures/zones.json")),
    (
        "equipments.json",
        include_str!("../fixtures/equipments.json"),
    ),
    ("recipes.json", include_str!("../fixtures/recipes.json")),
    (
        "sim-realistic.json",
        include_str!("../fixtures/sim-realistic.json"),
    ),
];
const SCENARIO: &str = include_str!("../fixtures/realistic.json");
const SCENARIO_NAME: &str = "realistic.json";
/// The simulated radio's memory file, `sim-<scenario name>.json`, as on the Mac.
const MEMORY_NAME: &str = "sim-realistic.json";
/// The scenario's clock runs this many times faster than the real one: 120 s in 12 s.
const SPEED: u32 = 10;
/// The engine's thread stack, in bytes (spec 016 R3.4).
pub const STACK_BYTES: usize = 32 * 1024;

/// The simulated radio, which sets `stop` once its scenario has played its last event.
struct Watched {
    inner: SimTransport<RealClock>,
    stop: Arc<AtomicBool>,
}

impl RadioTransport for Watched {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        let n = self.inner.read(buf, timeout_ms)?;
        if self.inner.finished() {
            self.stop.store(true, Ordering::SeqCst);
        }
        Ok(n)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.inner.write(bytes)
    }
}

/// Fills the buffer with the ESP32's random numbers.
#[allow(unsafe_code)]
fn fill(buffer: &mut [u8]) -> Result<(), String> {
    // SAFETY: `buffer` is valid for writes of `buffer.len()` bytes for the duration of the call,
    // which fills exactly that many bytes and has no other precondition.
    unsafe { esp_fill_random(buffer.as_mut_ptr().cast(), buffer.len()) };
    Ok(())
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Writes the realistic home when `/data` holds none. Called before the engine's memory is
/// measured: it belongs to the image, not to the engine.
pub fn seed() -> Result<(), String> {
    if FatStorage::new().read(DEVICES_FILE)?.is_some() {
        return Ok(());
    }
    for (name, text) in SEED {
        write_text(name, text)?;
    }
    log::info!("seed: realistic home written to {BASE}");
    Ok(())
}

/// Runs the scenario and measures it. `before` is the free heap just before the engine thread
/// started; its monitor of the local minimum is on.
pub fn run(before: usize) -> Result<EngineStats, String> {
    let storage = FatStorage::new();
    let start = Instant::now();
    let monotonic = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let clocks = Clocks {
        now: Arc::new(wall_ms),
        monotonic: Arc::new(monotonic),
    };
    let mut lows = Lows::new();
    let home = Mutex::new(load_home(&storage, wall_ms())?);
    lows.mark();
    lock(&home).start_recipes(monotonic(), wall_ms());
    let scenario = Scenario::parse(SCENARIO, SCENARIO_NAME)?;
    let memory_file = format!("{BASE}/{MEMORY_NAME}");
    let stop = Arc::new(AtomicBool::new(false));
    let transport = Watched {
        inner: SimTransport::for_scenario(
            &scenario,
            Some(std::path::Path::new(&memory_file)),
            SPEED,
            RealClock::new(),
        )?,
        stop: Arc::clone(&stop),
    };
    let mut radio = Radio::new(transport, move || {
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
    });
    lows.mark();
    radio.identify().map_err(|e| format!("radio: {e}"))?;
    radio.listen().map_err(|e| format!("radio: {e}"))?;
    lows.mark();
    let mut saver = Saver::new(FatStorage::new(), wall_ms());
    engine::run(
        &mut radio,
        &home,
        &mut saver,
        &clocks,
        &stop,
        &mut fill,
        false,
        &mut |line| log::info!("{line}"),
    )?;
    saver.tick(&home, wall_ms(), true)?;
    // The memory, with the home and the radio still alive.
    let after = measure::free_total();
    lows.mark();
    let now = memory::read();
    let guard = lock(&home);
    Ok(EngineStats {
        devices: guard.devices.list().len(),
        zones: guard.zones().list().len().saturating_sub(1),
        equipments: guard.equipments().list().len(),
        recipes: guard.recipes().len(),
        heap_used: before.saturating_sub(after),
        heap_peak: before.saturating_sub(lows.total),
        internal_free: now.internal_free,
        internal_min: lows.internal,
        psram_free: now.psram.map_or(0, |p| p.free),
        psram_min: lows.psram,
    })
}
