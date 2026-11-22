//! The engine on the real radio, for the radio image (feature `radio`, spec 021): one thread,
//! `engine`, pinned to core 1, owns the UART, the radio and the engine's loop, as `--serve` runs
//! them on the Mac without HTTP. It resets the radio, finds its serial speed, starts its network
//! (backup in `/data/zigbee.json`), opens pairing for a few minutes after a power-on boot, runs
//! the loop, restarts a wedged radio, and logs the serial buffer and the heap every few minutes.
//! The logic (probe, reset steps, boot window, restart, report timing and lines) is in the core,
//! tested on the Mac; this file gives it the hardware.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::sys::{
    MALLOC_CAP_INTERNAL, MALLOC_CAP_SPIRAM, bootloader_random_disable, bootloader_random_enable,
    esp_fill_random, esp_reset_reason, esp_reset_reason_t_ESP_RST_BROWNOUT,
    esp_reset_reason_t_ESP_RST_CPU_LOCKUP, esp_reset_reason_t_ESP_RST_EXT,
    esp_reset_reason_t_ESP_RST_INT_WDT, esp_reset_reason_t_ESP_RST_PANIC,
    esp_reset_reason_t_ESP_RST_POWERON, esp_reset_reason_t_ESP_RST_PWR_GLITCH,
    esp_reset_reason_t_ESP_RST_SW, esp_reset_reason_t_ESP_RST_TASK_WDT,
    esp_reset_reason_t_ESP_RST_USB, esp_reset_reason_t_ESP_RST_WDT,
    heap_caps_get_minimum_free_size, uxTaskGetStackHighWaterMark2,
};
use solite_core::HeapStats;
use solite_core::devices::lock;
use solite_core::engine::{self, Clocks, RunError, Saver, Storage, load_home};
use solite_core::heap_line;
use solite_core::radio::backup;
use solite_core::radio::serial::{
    BootPairing, RADIO_BAUD, ResetReason, RestartSteps, Restarted, Restarts, marker_wait_ms,
    pairing_at_boot, probe, probe_order, restart,
};
use solite_core::radio::{
    Network, NetworkSettings, Radio, RadioError, RadioTransport, TransportError,
};
use solite_core::radio::{identity_lines, network_line};

use crate::memory;
use crate::radio_uart::{UartTransport, reset_radio};
use crate::storage::{self, BASE, FatStorage};

/// The engine thread's stack, in bytes, in internal RAM (spec 021 R1.2).
pub const STACK_BYTES: usize = 32 * 1024;
/// The Zigbee backup, in the host's format.
const BACKUP_FILE: &str = "zigbee.json";
/// The marker written at each boot and removed after `STABLE_MS` of running.
const MARKER_FILE: &str = "booting";

type Clock = Box<dyn FnMut() -> u64 + Send>;
type Wired = Radio<Link, Clock>;

/// The milliseconds since `start`, as a clock.
fn clock(start: Instant) -> Clock {
    Box::new(move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX))
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Fills the buffer with the ESP32's random numbers.
#[allow(unsafe_code)]
fn fill(buffer: &mut [u8]) -> Result<(), String> {
    // SAFETY: `buffer` is valid for writes of `buffer.len()` bytes for the duration of the call,
    // which fills exactly that many bytes and has no other precondition.
    unsafe { esp_fill_random(buffer.as_mut_ptr().cast(), buffer.len()) };
    Ok(())
}

/// The heap's minimums since boot and the engine stack's high-water mark. Called from the engine
/// thread: the stack is the caller's.
#[allow(unsafe_code)]
fn heap_stats() -> HeapStats {
    let now = memory::read();
    // SAFETY: reads the heap's low-watermark counters for a capability mask, and the calling
    // task's stack high-water mark (a null handle means the caller); no precondition.
    let (internal_min, psram_min, stack_unused) = unsafe {
        (
            heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL),
            heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM),
            uxTaskGetStackHighWaterMark2(std::ptr::null_mut()),
        )
    };
    HeapStats {
        internal_free: now.internal_free,
        internal_min,
        psram_free: now.psram.map_or(0, |p| p.free),
        psram_min,
        stack_unused: stack_unused as usize,
        stack_total: STACK_BYTES,
    }
}

/// The UART, plus what runs at every read: the reports of the serial buffer and the heap.
/// Clones share the UART.
#[derive(Clone)]
struct Link {
    uart: UartTransport,
    start: Instant,
}

impl RadioTransport for Link {
    fn read(&mut self, buf: &mut [u8], timeout_ms: u64) -> Result<usize, TransportError> {
        let count = self.uart.read(buf, timeout_ms)?;
        let running = u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX);
        if let Some(line) = self.uart.serial_report(running) {
            log::info!("{line}");
            log::info!("{}", heap_line(&heap_stats()));
        }
        Ok(count)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.uart.write(bytes)
    }
}

/// Why the ESP32 started, from ESP-IDF's reset reason. The reasons are bindgen's constants,
/// named in lower case.
#[allow(unsafe_code, non_upper_case_globals)]
fn reset_reason() -> ResetReason {
    // SAFETY: takes no argument and only reads the reason recorded at boot.
    match unsafe { esp_reset_reason() } {
        esp_reset_reason_t_ESP_RST_POWERON
        | esp_reset_reason_t_ESP_RST_EXT
        | esp_reset_reason_t_ESP_RST_USB => ResetReason::PowerOn,
        esp_reset_reason_t_ESP_RST_SW => ResetReason::Software,
        esp_reset_reason_t_ESP_RST_PANIC | esp_reset_reason_t_ESP_RST_CPU_LOCKUP => {
            ResetReason::Panic
        }
        esp_reset_reason_t_ESP_RST_INT_WDT
        | esp_reset_reason_t_ESP_RST_TASK_WDT
        | esp_reset_reason_t_ESP_RST_WDT => ResetReason::Watchdog,
        esp_reset_reason_t_ESP_RST_BROWNOUT | esp_reset_reason_t_ESP_RST_PWR_GLITCH => {
            ResetReason::BrownOut
        }
        _ => ResetReason::Other,
    }
}

/// Logs a line of the core: the refusals and warnings as such, the rest as information.
fn show(line: &str) {
    if line.starts_with("pairing: failed")
        || line.contains("answered at")
        || line.contains("no answer at any")
        || line.contains("stopped after")
    {
        log::warn!("{line}");
    } else {
        log::info!("{line}");
    }
}

/// Writes the backup of a network just formed to `zigbee.json`; the backup of another network
/// already there is kept, renamed `zigbee-<PAN id>.json`, as the host keeps `network-<PAN id>.json`.
fn write_backup(network: &Network, keys: &solite_core::radio::NetworkKeys) -> Result<(), String> {
    if let Some(old) = FatStorage::new().read(BACKUP_FILE)?
        && !backup::is_network(&old, network)
    {
        // The old network's backup is the only copy of its keys: when it cannot be kept under its
        // own name, nothing is written over it.
        let kept = format!("zigbee-{}.json", backup::pan_label(&old));
        if let Err(e) = storage::rename(BACKUP_FILE, &kept) {
            log::error!("{e}: the backup of the new network is not written, {BACKUP_FILE} is kept");
            return Err(format!("cannot keep {BACKUP_FILE} as {kept}"));
        }
    }
    storage::write_text(BACKUP_FILE, &backup::text(network, keys))
}

/// The ESP32's random generator needs its entropy source on while no radio of the ESP32 runs;
/// it is on only while the network starts (forming draws the keys).
#[allow(unsafe_code)]
fn with_entropy<R>(body: impl FnOnce() -> R) -> R {
    // SAFETY: takes no argument; switches the hardware entropy source on, as the documentation
    // asks for random numbers without Wi-Fi or Bluetooth. Nothing else uses the ADC here.
    unsafe { bootloader_random_enable() };
    let result = body();
    // SAFETY: takes no argument; switches it off again, before any RF or ADC feature starts.
    unsafe { bootloader_random_disable() };
    result
}

/// Finds the serial speed (the core's probe), making a fresh radio at each one.
fn find_speed(link: &Link, start: Instant) -> Result<Option<u32>, RadioError> {
    let mut link = link.clone();
    let found = probe(
        &probe_order(RADIO_BAUD),
        |baud| {
            link.uart.set_baud(baud)?;
            let mut radio = Radio::new(&mut link, clock(start));
            radio.identify().map(|identity| (baud, identity))
        },
        &mut show,
    )?;
    Ok(found.map(|(baud, (_, identity))| {
        let lines = identity_lines("uart", &identity);
        // The `radio:` line of the probe replaces the first; the `network:` line is the one the
        // network's start gives.
        log::info!("{}", lines[1]);
        log::info!("{}", lines[2]);
        baud
    }))
}

/// A radio on the link, identified: the probe found the speed, this one is for the engine.
fn make_radio(link: &Link, start: Instant) -> Result<Wired, RadioError> {
    let mut radio = Radio::new(link.clone(), clock(start));
    radio.identify()?;
    Ok(radio)
}

/// Marks the boot, and decides whether pairing opens: the marker of an earlier boot within the
/// last `STABLE_MS` of running, or a reset that is not a power-on, says no.
fn boot_pairing() -> BootPairing {
    let marker = Path::new(BASE).join(MARKER_FILE);
    let present = marker.exists();
    if let Err(e) = std::fs::write(&marker, b"") {
        log::warn!("boot: cannot write the marker: {e}");
    }
    pairing_at_boot(present, reset_reason())
}

/// Starts the radio's network as `--pair` does on the Mac: resumed, or formed on channel 15 with
/// the backup written the moment the radio accepts it.
fn start_network(radio: &mut Wired) -> Result<(Network, bool), String> {
    let network =
        with_entropy(|| {
            radio.start_network(&NetworkSettings::default(), &mut fill, &mut |network| {
                match &network.keys {
                    Some(keys) => write_backup(network, keys),
                    None => Ok(()),
                }
            })
        })
        .map_err(|e| match e {
            // The backup's own message, as it is.
            RadioError::OnFormed(message) => message,
            other => format!("radio: {other}"),
        })?;
    let backed_up = FatStorage::new()
        .read(BACKUP_FILE)
        .ok()
        .flatten()
        .is_some_and(|text| backup::is_network(&text, &network));
    let foreign = network.keys.is_none() && !backed_up;
    Ok((network, foreign))
}

fn run() -> Result<(), String> {
    let start = Instant::now();
    let mono = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let link = Link {
        uart: UartTransport::install(0)?,
        start,
    };
    let home = Mutex::new(load_home(&FatStorage::new(), wall_ms())?);
    lock(&home).start_recipes(mono(), wall_ms());
    let mut saver = Saver::new(FatStorage::new(), wall_ms());
    let boot = boot_pairing();

    reset_radio()?;
    // No answer at any speed: the probe said so; the engine does not start and the ESP32 stays up.
    if find_speed(&link, start)
        .map_err(|e| format!("radio: {e}"))?
        .is_none()
    {
        return Ok(());
    }
    let mut radio = make_radio(&link, start).map_err(|e| format!("radio: {e}"))?;
    let (network, foreign) = start_network(&mut radio)?;
    // Joining stays closed until the window opens, and devices are only followed while it is open.
    radio.set_following(false);
    log::info!("{}", network_line(&network, foreign));
    match boot {
        BootPairing::Open { seconds } => lock(&home).pairing_mut().request(seconds),
        other => {
            if let Some(line) = other.line() {
                show(&line);
            }
        }
    }

    let clocks = Clocks {
        now: Arc::new(wall_ms),
        monotonic: Arc::new(mono),
    };
    let stop = AtomicBool::new(false);
    let mut restarts = Restarts::new();
    let mut slot: Option<Wired> = Some(radio);
    loop {
        let Some(radio) = slot.as_mut() else {
            return Ok(());
        };
        let error = match engine::run(
            radio,
            &home,
            &mut saver,
            &clocks,
            &stop,
            &mut fill,
            true,
            &mut |line| show(line),
        ) {
            Ok(_) => return Ok(()),
            // A storage error, or a failed measure: logged by the caller, the loop stops and the
            // radio is left as it is.
            Err(RunError::Other(message)) => return Err(message),
            Err(RunError::Radio(error)) => error,
        };
        // A radio error restarts the radio, a few times: each failed attempt counts again.
        let mut last = error.to_string();
        loop {
            let mut save = || saver.tick(&home, wall_ms(), false);
            let mut finish = || lock(&home).pairing_mut().finished();
            let mut reset = || {
                if let Err(e) = reset_radio() {
                    log::error!("{e}");
                }
            };
            let mut find = || find_speed(&link, start);
            let mut listen = || -> Result<Network, RadioError> {
                let mut radio = make_radio(&link, start)?;
                let network = radio.listen()?;
                radio.set_following(false);
                slot = Some(radio);
                Ok(network)
            };
            let mut steps = RestartSteps {
                save: &mut save,
                finish_pairing: &mut finish,
                reset: &mut reset,
                probe: &mut find,
                listen: &mut listen,
            };
            match restart(&mut restarts, mono(), &last, &mut steps, &mut |line| {
                show(line)
            }) {
                Ok(Restarted::Resumed(_)) => break,
                // The line says why; the ESP32 stays up.
                Ok(Restarted::Stopped) => return Ok(()),
                Err(error) => last = error.to_string(),
            }
        }
    }
}

/// Removes the boot marker after `STABLE_MS` of running, on its own timer: a storage error, a radio
/// that answers at no speed or a radio stopped after its restarts end the engine's loop, not the
/// ESP32's run, and must not leave the marker for the next power-on.
fn start_marker_timer() {
    let spawned = std::thread::Builder::new()
        .name("marker".into())
        .stack_size(4096)
        .spawn(|| {
            std::thread::sleep(Duration::from_millis(marker_wait_ms(0)));
            match std::fs::remove_file(Path::new(BASE).join(MARKER_FILE)) {
                Ok(()) => log::info!("boot: marker removed after 10 min of running"),
                Err(e) => log::warn!("boot: cannot remove the marker: {e}"),
            }
        });
    if let Err(e) = spawned {
        log::warn!("boot: cannot start the marker timer: {e}");
    }
}

/// The engine thread's body: any error is logged by the caller, the ESP32 stays up.
fn engine_thread() {
    start_marker_timer();
    if let Err(e) = run() {
        log::error!("{e}");
    }
}

/// Starts the engine thread on core 1, with `STACK_BYTES` of stack in internal RAM.
pub fn spawn() -> Result<JoinHandle<()>, String> {
    ThreadSpawnConfiguration {
        name: Some(c"engine"),
        stack_size: STACK_BYTES,
        pin_to_core: Some(Core::Core1),
        ..Default::default()
    }
    .set()
    .map_err(|e| format!("engine: cannot configure its thread: {e}"))?;
    let handle = std::thread::Builder::new()
        .name("engine".into())
        .stack_size(STACK_BYTES)
        .spawn(engine_thread)
        .map_err(|e| e.to_string());
    // Threads started later get the default configuration again.
    let _ = ThreadSpawnConfiguration::default().set();
    handle
}
