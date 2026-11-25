//! The engine on the real radio, for the radio image (feature `radio`, spec 021): one thread,
//! `engine`, pinned to core 1, owns the UART, the radio and the engine's loop, as `--serve` runs
//! them on the Mac without HTTP. It resets the radio, finds its serial speed, starts its network
//! (backup in `/data/zigbee.json`), opens pairing for a few minutes after a power-on boot, runs
//! the loop, restarts a wedged radio, and logs the serial buffer and the heap every few minutes.
//! The thread is on the task watchdog; an error that ends it (radio that stays silent or
//! stopped, no radio start, an engine error) goes through the supervisor's last rung: the files
//! are saved and the ESP32 restarts with the reason kept (spec 036).
//! With Wi-Fi (spec 023) it hands the loaded home and its live hub to `main`, which serves them;
//! its saves wait for the clock (the saver's gate), and a stop asked by the console saves the
//! files before the ESP32 restarts. The logic (probe, reset steps, boot window, restart, report timing and lines) is in the core,
//! tested on the Mac; this file gives it the hardware.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
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
    heap_caps_get_largest_free_block, heap_caps_get_minimum_free_size,
    uxTaskGetStackHighWaterMark2,
};
use solite_core::HeapStats;
use solite_core::auth::BootPresence;
use solite_core::backup::BackupFiles;
use solite_core::devices::lock;
use solite_core::engine::{
    self, Clocks, DEFINITION_DELAY_MS, RunError, SaveControl, Saver, Storage, load_home,
};
use solite_core::heap_line;
use solite_core::home::Home;
use solite_core::live::LiveHandle;
use solite_core::radio::backup;
use solite_core::radio::serial::{
    BootPairing, PAIR_AT_BOOT_SECONDS, RADIO_BAUD, ResetReason, RestartSteps, Restarted, Restarts,
    marker_wait_ms, pairing_at_boot, probe, probe_order, restart,
};
use solite_core::radio::{
    Network, NetworkSettings, Radio, RadioError, RadioTransport, TransportError,
};
use solite_core::radio::{identity_lines, network_line};
use solite_core::supervisor::Reason;

use crate::backup_files::data_files;
use crate::http_server;
use crate::memory;
use crate::ota::ENGINE_RUNNING;
use crate::radio_uart::{UartTransport, reset_radio};
use crate::storage::{self, BASE, FatStorage};
use crate::supervisor;
use crate::wifi::{CLOCK_SET, Event, WIFI_STARTED};

/// The engine thread's stack, in bytes, in internal RAM (spec 021 R1.2).
pub const STACK_BYTES: usize = 32 * 1024;
/// The Zigbee backup, in the host's format.
const BACKUP_FILE: &str = "zigbee.json";
/// Set by the console's `pair` line: the engine opens the pairing window for the same time as at a
/// power-on boot, whatever the boot was (taken by `start_pair_watch`).
pub static PAIR_ASKED: AtomicBool = AtomicBool::new(false);

/// The saves held and flushed by the console's `saves hold` and `saves flush` (spec 025): in RAM
/// only, so every boot starts with saves not held.
pub static SAVES: SaveControl = SaveControl::new();

/// The marker written at each boot and removed after `STABLE_MS` of running.
const MARKER_FILE: &str = "booting";

/// Set by the console to stop the engine (its files are then saved) before a restart.
pub static STOP: AtomicBool = AtomicBool::new(false);
/// Set when the engine thread has ended, stopped or not.
pub static STOPPED: AtomicBool = AtomicBool::new(false);

/// What the engine hands to `main` once the home is loaded: the server shares both.
pub struct Shared {
    pub home: Arc<Mutex<Home>>,
    pub hub: LiveHandle,
}

type Clock = Box<dyn FnMut() -> u64 + Send>;
type Wired = Radio<Link, Clock>;

/// The milliseconds since `start`, as a clock.
fn clock(start: Instant) -> Clock {
    Box::new(move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX))
}

/// The time since 1970, in milliseconds (from 1970 itself until the clock is set).
pub fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Fills the buffer with the ESP32's random numbers.
#[allow(unsafe_code)]
pub fn fill(buffer: &mut [u8]) -> Result<(), String> {
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
    // SAFETY: reads the heap's low-watermark counters and largest free blocks for a capability
    // mask, and the calling task's stack high-water mark (a null handle means the caller); no precondition.
    let (internal_min, psram_min, internal_largest, psram_largest, stack_unused) = unsafe {
        (
            heap_caps_get_minimum_free_size(MALLOC_CAP_INTERNAL),
            heap_caps_get_minimum_free_size(MALLOC_CAP_SPIRAM),
            heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL),
            heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM),
            uxTaskGetStackHighWaterMark2(std::ptr::null_mut()),
        )
    };
    HeapStats {
        internal_free: now.internal_free,
        internal_min,
        internal_largest,
        psram_free: now.psram.map_or(0, |p| p.free),
        psram_min,
        psram_largest,
        stack_unused: stack_unused as usize,
        stack_total: STACK_BYTES,
        http_stack: http_server::http_stack(),
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
        // Every read of the radio feeds the engine's watchdog: a slice is far shorter than its
        // timeout (spec 036 R8).
        supervisor::feed();
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
pub fn reset_reason() -> ResetReason {
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
/// it is on only while the network starts (forming draws the keys). Once the Wi-Fi station has
/// started, its radio feeds the generator and the switch must not run (spec 023 R3.4).
#[allow(unsafe_code)]
fn with_entropy<R>(body: impl FnOnce() -> R) -> R {
    if WIFI_STARTED.load(Ordering::SeqCst) {
        return body();
    }
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
    if let Err(e) = storage::touch(MARKER_FILE) {
        log::warn!("boot: cannot write the marker: {e}");
    }
    pairing_at_boot(present, reset_reason())
}

/// Starts the radio's network as `--pair` does on the Mac: resumed, or formed on channel 15 with
/// the backup written the moment the radio accepts it.
fn start_network(radio: &mut Wired) -> Result<(Network, bool), String> {
    // The restored network (spec 035): the file's values when the radio has none, or, with the mark
    // a restore left, the radio is moved onto it.
    let mut files = data_files();
    let restored = FatStorage::new()
        .read(BACKUP_FILE)
        .ok()
        .flatten()
        .and_then(|text| {
            let parsed = backup::parse(&text);
            if parsed.is_none() {
                log::warn!("backup: {BACKUP_FILE} cannot be read, a network is formed as usual");
            }
            parsed
        });
    let replace = files.restore_mark()?;
    let network = with_entropy(|| {
        radio.start_network(
            &NetworkSettings::default(),
            restored.as_ref(),
            replace,
            &mut fill,
            &mut |network| match &network.keys {
                Some(keys) => write_backup(network, keys),
                None => Ok(()),
            },
        )
    })
    .map_err(|e| match e {
        // The backup's own message, as it is.
        RadioError::OnFormed(message) => message,
        other => format!("radio: {other}"),
    })?;
    if replace {
        files.clear_restore_mark()?;
    }
    let backed_up = FatStorage::new()
        .read(BACKUP_FILE)
        .ok()
        .flatten()
        .is_some_and(|text| backup::is_network(&text, &network));
    let foreign = network.keys.is_none() && !backed_up;
    Ok((network, foreign))
}

/// Why the engine's run ended in error: the reason the supervisor keeps, the message, and
/// whether the supervisor's last rung already ran for it (the files saved, the restart asked).
struct Failure {
    reason: Reason,
    message: String,
    handled: bool,
}

impl Failure {
    fn new(reason: Reason, message: impl Into<String>) -> Failure {
        Failure {
            reason,
            message: message.into(),
            handled: false,
        }
    }
}

/// Start-up errors of the radio: `RadioStart`.
fn radio_start(message: impl Into<String>) -> Failure {
    Failure::new(Reason::RadioStart, message)
}

/// `home_sent` turns true once `main` has the home (its server is then up), so that a later
/// error is not reported as "no home". An error that ends the run is a `Failure`; the last rung
/// (save, restart with the reason kept) is run before it returns, once the home exists.
fn run(events: Option<&Sender<Event>>, home_sent: &mut bool) -> Result<(), Failure> {
    let start = Instant::now();
    let mono = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let link = Link {
        uart: UartTransport::install(0).map_err(radio_start)?,
        start,
    };
    let home = Arc::new(Mutex::new(
        load_home(&FatStorage::new(), wall_ms())
            .map_err(|e| Failure::new(Reason::EngineError, e))?,
    ));
    {
        // The recipes' timers and the orders' deadlines run on the same monotonic clock from the
        // first moment, API calls included (as `crates/host/src/serve.rs` does).
        let mut guard = lock(&home);
        guard.orders().set_clock(Arc::new(mono));
        // The box asks for a first password only within ten minutes of a power-on (spec 029 R6):
        // a software restart or a crash never opens the setup to the home's network.
        guard.set_presence(BootPresence::new(
            reset_reason() == ResetReason::PowerOn,
            Arc::new(mono),
        ));
        supervisor::set_supervision(&mut guard);
        guard.start_recipes(mono(), wall_ms());
    }
    supervisor::watch_home(&home);
    if let Some(events) = events {
        let hub = lock(&home).live();
        *home_sent = events
            .send(Event::Home(Shared {
                home: Arc::clone(&home),
                hub,
            }))
            .is_ok();
    }
    // The saves wait for the clock (spec 023 R3.2); a stop writes whatever the gate says. The
    // console holds and flushes them, and a burst of changes gives one write (spec 025).
    let mut saver = Saver::new(FatStorage::new(), wall_ms())
        .with_gate(|| CLOCK_SET.load(Ordering::SeqCst))
        .with_control(&SAVES)
        .with_definition_delay(DEFINITION_DELAY_MS)
        .with_feed(supervisor::feed);
    // From here the files can be saved by the last rung.
    let result = drive(&link, &home, &mut saver, start);
    match result {
        Ok(()) => Ok(()),
        Err(mut failure) => {
            log::error!("{}", failure.message);
            let mut save = || saver.tick(&home, wall_ms(), true);
            supervisor::last_rung_restart(failure.reason, &failure.message, &mut save);
            failure.handled = true;
            Err(failure)
        }
    }
}

/// The radio's start and the engine's loop, after the home is loaded and the saver made.
fn drive(
    link: &Link,
    home: &Arc<Mutex<Home>>,
    saver: &mut Saver<FatStorage>,
    start: Instant,
) -> Result<(), Failure> {
    let mono = move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let boot = boot_pairing();

    reset_radio().map_err(radio_start)?;
    // No answer at any speed: the probe said so; the box restarts (the last rung) and tries
    // again, up to the supervisor's safe mode.
    if find_speed(link, start)
        .map_err(|e| radio_start(format!("radio: {e}")))?
        .is_none()
    {
        return Err(Failure::new(
            Reason::RadioSilent,
            "radio: no answer at any speed",
        ));
    }
    let mut radio = make_radio(link, start).map_err(|e| radio_start(format!("radio: {e}")))?;
    let (network, foreign) = start_network(&mut radio).map_err(radio_start)?;
    // Joining stays closed until the window opens, and devices are only followed while it is open.
    radio.set_following(false);
    log::info!("{}", network_line(&network, foreign));
    match boot {
        BootPairing::Open { seconds } => lock(home).pairing_mut().request(seconds),
        other => {
            if let Some(line) = other.line() {
                show(&line);
            }
        }
    }

    start_pair_watch(home);
    let clocks = Clocks {
        now: Arc::new(wall_ms),
        monotonic: Arc::new(mono),
    };
    let stop = &STOP;
    let mut restarts = Restarts::new();
    let mut slot: Option<Wired> = Some(radio);
    loop {
        let Some(radio) = slot.as_mut() else {
            return Err(Failure::new(
                Reason::RadioStopped,
                "radio: none after a restart",
            ));
        };
        // The update's probation counts the engine's loop as healthy while it runs (spec 032 R5).
        ENGINE_RUNNING.store(true, Ordering::SeqCst);
        let ran = engine::run(
            radio,
            home,
            saver,
            &clocks,
            stop,
            &mut fill,
            true,
            &mut |line| show(line),
        );
        ENGINE_RUNNING.store(false, Ordering::SeqCst);
        let error = match ran {
            // Stopped (the console's restart): the files due are saved first.
            Ok(_) => {
                return saver
                    .tick(home, wall_ms(), true)
                    .map_err(|e| Failure::new(Reason::EngineError, e));
            }
            // A storage error, or a failed measure: the last rung saves what it can and restarts
            // the box.
            Err(RunError::Other(message)) => {
                return Err(Failure::new(Reason::EngineError, message));
            }
            Err(RunError::Radio(error)) => error,
        };
        // A radio error restarts the radio, a few times: each failed attempt counts again.
        let mut last = error.to_string();
        loop {
            let mut save = || saver.tick(home, wall_ms(), false);
            let mut finish = || lock(home).pairing_mut().finished();
            let mut reset = || {
                if let Err(e) = reset_radio() {
                    log::error!("{e}");
                }
            };
            let mut find = || find_speed(link, start);
            let mut listen = || -> Result<Network, RadioError> {
                let mut radio = make_radio(link, start)?;
                let network = radio.listen()?;
                radio.set_following(false);
                slot = Some(radio);
                Ok(network)
            };
            let mut feed = supervisor::feed;
            let mut steps = RestartSteps {
                feed: &mut feed,
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
                // The line says why; the last rung restarts the ESP32.
                Ok(Restarted::Stopped) => {
                    return Err(Failure::new(
                        Reason::RadioStopped,
                        "radio: stopped after its restarts",
                    ));
                }
                Ok(Restarted::Silent) => {
                    return Err(Failure::new(
                        Reason::RadioSilent,
                        "radio: no answer at any speed",
                    ));
                }
                Err(error) => last = error.to_string(),
            }
        }
    }
}

/// Turns the console's `pair` line into a pairing request, every half second.
fn start_pair_watch(home: &Arc<Mutex<Home>>) {
    let home = Arc::clone(home);
    let spawned = std::thread::Builder::new()
        .name("pair-watch".into())
        .stack_size(4096)
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(500));
                if PAIR_ASKED.swap(false, Ordering::SeqCst) {
                    lock(&home).pairing_mut().request(PAIR_AT_BOOT_SECONDS);
                }
            }
        });
    if let Err(e) = spawned {
        log::warn!("pairing: cannot start the console watch: {e}");
    }
}

/// Removes the boot marker after `STABLE_MS` of running, on its own timer: the marker says "this
/// run was stable". An error that ends the engine's loop restarts the ESP32 (spec 036), long
/// before the marker is due, so the next boot finds it and opens no pairing window.
fn start_marker_timer() {
    let spawned = std::thread::Builder::new()
        .name("marker".into())
        .stack_size(4096)
        .spawn(|| {
            std::thread::sleep(Duration::from_millis(marker_wait_ms(0)));
            match storage::remove(MARKER_FILE) {
                Ok(()) => log::info!("boot: marker removed after 10 min of running"),
                Err(e) => log::warn!("boot: cannot remove the marker: {e}"),
            }
        });
    if let Err(e) = spawned {
        log::warn!("boot: cannot start the marker timer: {e}");
    }
}

/// The engine thread's body, on the task watchdog. Any error is logged and ends in the
/// supervisor's last rung: the files are saved and the ESP32 restarts with the reason kept (spec
/// 036). Without a home, `main` is told why first (no server starts). The last rung returns only
/// when a stop was asked meanwhile: the asked restart wins.
fn engine_thread(events: Option<Sender<Event>>) {
    start_marker_timer();
    supervisor::subscribe();
    let mut home_sent = false;
    if let Err(failure) = run(events.as_ref(), &mut home_sent)
        && !failure.handled
    {
        // No home yet, so nothing to save.
        log::error!("{}", failure.message);
        if let Some(events) = events.as_ref().filter(|_| !home_sent) {
            let _ = events.send(Event::NoHome(failure.message.clone()));
        }
        supervisor::last_rung_restart(failure.reason, &failure.message, &mut || Ok(()));
    }
    supervisor::unsubscribe();
    STOPPED.store(true, Ordering::SeqCst);
}

/// Starts the engine thread on core 1, with `STACK_BYTES` of stack in internal RAM. With Wi-Fi,
/// `events` receives the shared home once it is loaded.
pub fn spawn(events: Option<Sender<Event>>) -> Result<JoinHandle<()>, String> {
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
        .spawn(move || engine_thread(events))
        .map_err(|e| e.to_string());
    // Threads started later get the default configuration again.
    let _ = ThreadSpawnConfiguration::default().set();
    handle
}
