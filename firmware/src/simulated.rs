//! The engine on the simulated radio, for the image QEMU runs (feature `sim`, spec 016): the
//! realistic home of `fixtures/` is written to `/data` when it holds no `devices.json` (`seed`),
//! then the engine plays the fixture scenario, in its own thread, as `--sim --serve` does on the Mac but
//! without HTTP. The scenario waits, held, until the radio is identified and listening and the
//! loop has turned once; its events then play at their own pace, with loop turns between them.
//! When the scenario has played its last event, the loop stops, the saver writes, and the memory
//! of the run is measured.
//!
//! With the cargo feature `flash-saves` (the measuring image, spec 020) every save goes to the
//! flash and is timed: the time of a `Storage::write` call is the time the radio loop is held. The
//! run logs `saves: <n> written to flash, longest <ms> ms (<file>, <KB> KB), total <ms> ms`.

use std::cell::Cell;
#[cfg(not(feature = "flash-saves"))]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use esp_idf_svc::sys::esp_fill_random;
use solite_core::EngineStats;
use solite_core::devices::lock;
use solite_core::engine::{self, Clocks, DEVICES_FILE, Saver, Storage, load_home};
use solite_core::radio::{Radio, RadioTransport, TransportError};
use solite_sim::{Clock, Scenario, SimTransport};

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

/// The simulator's clock: it counts the time the simulator really waits (a sleep), nothing
/// else. The emulator writes the flash far slower than the box does (a save of the home takes
/// seconds), and wall time spent there would play the scenario's events in one burst; the
/// events come at their own pace, one loop turn after the other.
struct WaitedClock {
    waited_ms: Cell<u64>,
}

impl Clock for WaitedClock {
    fn now_ms(&self) -> u64 {
        self.waited_ms.get()
    }

    fn wait_ms(&self, ms: u64) {
        // The time that really passed: the RTOS rounds a short sleep up to its tick (10 ms).
        let begin = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let passed = u64::try_from(begin.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.waited_ms
            .set(self.waited_ms.get().saturating_add(passed.max(ms)));
    }
}

/// The engine's files while the scenario plays: reads go to the flash; a save is run to its end
/// (the home is walked chunk by chunk, as for the flash) and its text dropped, until `to_flash`
/// is set. The emulator writes the flash about 20 s for the 94 KB of `devices.json`, and the
/// engine saves it once per device when its route is learned (the fixtures carry no route, so
/// each of the 50 devices sets one at its first message); written for real, 49 saves would take
/// a quarter of an hour. The last save, on stop, goes to the flash, so that its path is run for real.
/// With the feature `flash-saves` (the box) every save goes to the flash, timed.
struct RunStorage {
    inner: FatStorage,
    #[cfg(not(feature = "flash-saves"))]
    to_flash: Arc<AtomicBool>,
    #[cfg(not(feature = "flash-saves"))]
    dropped: Arc<AtomicUsize>,
    #[cfg(feature = "flash-saves")]
    stats: Arc<Mutex<SaveStats>>,
}

/// What the saves cost on the box: a count, the total time and the longest save, no list (the
/// file name is one of the engine's four).
#[cfg(feature = "flash-saves")]
#[derive(Default)]
struct SaveStats {
    count: usize,
    total_ms: u64,
    longest_ms: u64,
    longest_file: String,
    longest_bytes: usize,
}

impl Storage for RunStorage {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        self.inner.read(name)
    }

    #[cfg(not(feature = "flash-saves"))]
    fn write(
        &self,
        name: &str,
        next: &mut dyn FnMut() -> std::io::Result<Option<String>>,
    ) -> Result<(), String> {
        if self.to_flash.load(Ordering::SeqCst) {
            return self.inner.write(name, next);
        }
        while next()
            .map_err(|e| format!("{name}: cannot write: {e}"))?
            .is_some()
        {}
        self.dropped.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[cfg(feature = "flash-saves")]
    fn write(
        &self,
        name: &str,
        next: &mut dyn FnMut() -> std::io::Result<Option<String>>,
    ) -> Result<(), String> {
        let begin = Instant::now();
        let mut bytes = 0usize;
        let result = self.inner.write(name, &mut || {
            let chunk = next()?;
            bytes += chunk.as_ref().map_or(0, String::len);
            Ok(chunk)
        });
        let ms = u64::try_from(begin.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut stats = lock(&self.stats);
        stats.count += 1;
        stats.total_ms = stats.total_ms.saturating_add(ms);
        if ms >= stats.longest_ms {
            stats.longest_ms = ms;
            stats.longest_file = name.to_string();
            stats.longest_bytes = bytes;
        }
        result
    }

    fn locate(&self, name: &str) -> String {
        self.inner.locate(name)
    }
}

/// The most devices whose first measure is counted: the home's bound is far below.
const MAX_REPORTERS: usize = 128;

/// The simulated radio, which starts its scenario when `go` is set (at the loop's first turn) and
/// sets `stop` once the scenario has played its last event.
struct Watched {
    inner: SimTransport<WaitedClock>,
    go: Arc<AtomicBool>,
    started: bool,
    stop: Arc<AtomicBool>,
}

impl RadioTransport for Watched {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        if !self.started && self.go.load(Ordering::SeqCst) {
            self.started = true;
            self.inner.release();
            log::info!("scenario: started");
        }
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
pub fn run(before: usize) -> Result<(), String> {
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
    let mut inner = SimTransport::for_scenario(
        &scenario,
        Some(std::path::Path::new(&memory_file)),
        SPEED,
        WaitedClock {
            waited_ms: Cell::new(0),
        },
    )?;
    // The simulated radio's own memory stays in RAM once read: the emulator writes the flash far
    // slower than the box, and the radio's flash is not what this image measures.
    inner.keep_memory_in_ram();
    inner.hold();
    let go = Arc::new(AtomicBool::new(false));
    let transport = Watched {
        inner,
        go: Arc::clone(&go),
        started: false,
        stop: Arc::clone(&stop),
    };
    let mut radio = Radio::new(transport, move || {
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
    });
    lows.mark();
    radio.identify().map_err(|e| format!("radio: {e}"))?;
    radio.listen().map_err(|e| format!("radio: {e}"))?;
    lows.mark();
    #[cfg(not(feature = "flash-saves"))]
    let to_flash = Arc::new(AtomicBool::new(false));
    #[cfg(not(feature = "flash-saves"))]
    let dropped = Arc::new(AtomicUsize::new(0));
    #[cfg(feature = "flash-saves")]
    let stats = Arc::new(Mutex::new(SaveStats::default()));
    let mut saver = Saver::new(
        RunStorage {
            inner: FatStorage::new(),
            #[cfg(not(feature = "flash-saves"))]
            to_flash: Arc::clone(&to_flash),
            #[cfg(not(feature = "flash-saves"))]
            dropped: Arc::clone(&dropped),
            #[cfg(feature = "flash-saves")]
            stats: Arc::clone(&stats),
        },
        wall_ms(),
    );
    // The devices that got a measure into the engine: each measure line starts with the device's
    // address (`0x00124B0012345600 temperature 18.0`). At most `MAX_REPORTERS` kept.
    let mut reporters: Vec<u64> = Vec::new();
    let mut count = |line: &str| {
        let address = line
            .split(' ')
            .next()
            .and_then(|a| a.strip_prefix("0x"))
            .and_then(|a| u64::from_str_radix(a, 16).ok());
        if let Some(address) = address
            && reporters.len() < MAX_REPORTERS
            && !reporters.contains(&address)
        {
            reporters.push(address);
            log::info!("scenario: {} devices reported", reporters.len());
        }
    };
    go.store(true, Ordering::SeqCst);
    engine::run(
        &mut radio, &home, &mut saver, &clocks, &stop, &mut fill, true, &mut count,
    )?;
    #[cfg(not(feature = "flash-saves"))]
    {
        log::info!(
            "scenario: {} saves run without the emulator's flash",
            dropped.load(Ordering::SeqCst)
        );
        to_flash.store(true, Ordering::SeqCst);
    }
    saver.tick(&home, wall_ms(), true)?;
    #[cfg(feature = "flash-saves")]
    {
        let stats = lock(&stats);
        log::info!(
            "saves: {} written to flash, longest {} ms ({}, {} KB), total {} ms",
            stats.count,
            stats.longest_ms,
            stats.longest_file,
            stats.longest_bytes.div_ceil(1024),
            stats.total_ms
        );
    }
    // The FAT partition after the last save: the figure of the realistic home.
    match crate::storage::usage() {
        Ok((used, total)) => log::info!(
            "storage-after: {}, {} KB used of {} KB",
            BASE,
            used / 1024,
            total / 1024
        ),
        Err(e) => log::warn!("storage-after: {e}"),
    }
    // The memory, with the home and the radio still alive.
    let after = measure::free_total();
    lows.mark();
    let now = memory::read();
    let stats = {
        let guard = lock(&home);
        EngineStats {
            devices: guard.devices.list().len(),
            reported: reporters.len(),
            zones: guard.zones().list().len().saturating_sub(1),
            equipments: guard.equipments().list().len(),
            recipes: guard.recipes().len(),
            heap_used: before.saturating_sub(after),
            heap_peak: before.saturating_sub(lows.total),
            internal_free: now.internal_free,
            internal_min: lows.internal,
            psram_free: now.psram.map_or(0, |p| p.free),
            psram_min: lows.psram,
        }
    };
    log::info!("{}", solite_core::engine_line(&stats));
    // After the engine's line, which the scripts never touch (decision D27): the script phase.
    #[cfg(feature = "scripts")]
    script_phase::run(&home, &clocks, start)?;
    Ok(())
}

/// The script phase of the image QEMU runs (feature `scripts`, spec 041 R6.2).
#[cfg(feature = "scripts")]
#[path = "script_phase.rs"]
mod script_phase;
