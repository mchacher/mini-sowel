//! The radio image's serial console (feature `radio`, spec 023 R6): a thread on core 0 reads the
//! lines `scripts/box.sh wifi` sends on the USB serial port (UART0, the log's port), hands each to
//! the core's `parse_console_line`, writes or removes `/data/wifi.json`, then stops the engine
//! (its files saved) and restarts the ESP32. A line itself is never logged.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::sys::{
    TickType_t, configTICK_RATE_HZ, esp_restart, uart_driver_install, uart_read_bytes,
    uart_vfs_dev_use_driver, uxTaskGetStackHighWaterMark2,
};
use solite_core::network::{ConsoleCommand, MAX_CONSOLE_LINE, parse_console_line};

use crate::radio_engine::{STOP, STOPPED};
use crate::storage;

/// The console thread's stack, in internal RAM: it writes the flash.
const CONSOLE_STACK_BYTES: usize = 4 * 1024;
/// The receive ring of UART0, in internal RAM (more than its 128-byte hardware FIFO).
const RX_RING_BYTES: i32 = 512;
/// The ESP32 restarts this long after asking the engine to stop, stopped or not.
const RESTART_WAIT_MS: u64 = 10_000;
/// One read waits this long for a byte, then the loop reads again: an unbounded wait
/// (`portMAX_DELAY`) overflows the driver's own deadline arithmetic.
const READ_WAIT_MS: TickType_t = 100;
/// The Wi-Fi settings' file, in the data partition.
pub const WIFI_FILE: &str = "wifi.json";
/// UART0: ESP-IDF's console and the log's port.
const CONSOLE_UART: u32 = 0;

/// Installs UART0's driver, so that reads block, and moves the console's writes onto it.
#[allow(unsafe_code)]
fn install() -> Result<(), String> {
    // SAFETY: UART0 has no driver yet (ESP-IDF's console writes without one); no queue is asked
    // for (null), no interrupt flag. The VFS switch makes the log's writes go through the same
    // driver, so the two never use the port at once.
    let code =
        unsafe { uart_driver_install(CONSOLE_UART, RX_RING_BYTES, 0, 0, std::ptr::null_mut(), 0) };
    if code != 0 {
        return Err(format!(
            "console: cannot install the UART driver (error {code})"
        ));
    }
    // SAFETY: the driver of UART0 is installed just above.
    unsafe { uart_vfs_dev_use_driver(CONSOLE_UART as i32) };
    Ok(())
}

/// Reads the bytes that came, waiting for at least one.
#[allow(unsafe_code)]
fn read(buffer: &mut [u8]) -> usize {
    // SAFETY: the driver is installed; ESP-IDF writes at most `buffer.len()` bytes into it.
    let count = unsafe {
        uart_read_bytes(
            CONSOLE_UART,
            buffer.as_mut_ptr().cast(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
            READ_WAIT_MS * configTICK_RATE_HZ / 1000,
        )
    };
    usize::try_from(count).unwrap_or(0)
}

/// Stops the engine (its files saved by its forced save), waits for it at most
/// `RESTART_WAIT_MS`, and restarts the ESP32 by software (no pairing window: not a power-on).
#[allow(unsafe_code)]
fn restart() -> ! {
    STOP.store(true, Ordering::SeqCst);
    let asked = Instant::now();
    while !STOPPED.load(Ordering::SeqCst)
        && asked.elapsed() < Duration::from_millis(RESTART_WAIT_MS)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    if !STOPPED.load(Ordering::SeqCst) {
        log::warn!("console: the engine did not stop within 10 s, restarting anyway");
    }
    // The thread's stack margin after its flash write, read on the box (spec 023's plan).
    // SAFETY: a null handle means the calling task; only reads its high-water mark.
    let unused = unsafe { uxTaskGetStackHighWaterMark2(std::ptr::null_mut()) } as usize;
    log::info!(
        "console: restarting (stack {} B unused of {CONSOLE_STACK_BYTES} B)",
        unused
    );
    // SAFETY: takes no argument; restarts the chip, never returns.
    unsafe { esp_restart() }
}

/// What one line asks for, done.
fn act(line: &[u8]) {
    match parse_console_line(line) {
        ConsoleCommand::Empty => {}
        ConsoleCommand::TooLong => {
            log::warn!("console: line over {MAX_CONSOLE_LINE} bytes, ignored");
        }
        ConsoleCommand::Unknown => log::warn!("console: unknown command"),
        ConsoleCommand::Refused(refusal) => {
            log::warn!("wifi: settings refused ({})", refusal.message());
        }
        ConsoleCommand::WifiSet(settings) => {
            // The core's text, read back to the same settings: the line is not stored as is.
            match storage::write_text(WIFI_FILE, &settings.to_json()) {
                Ok(()) => {
                    log::info!("wifi: settings saved");
                    restart();
                }
                Err(e) => log::error!("{e}"),
            }
        }
        ConsoleCommand::WifiForget => match storage::remove(WIFI_FILE) {
            Ok(()) => {
                log::info!("wifi: settings removed");
                restart();
            }
            Err(e) => log::error!("{e}"),
        },
    }
}

/// The thread's body: lines of at most `MAX_CONSOLE_LINE` + 1 bytes kept (the rest of a longer
/// line discarded up to its `\n`, which the core then refuses as too long).
fn console_loop() {
    let mut line = [0u8; MAX_CONSOLE_LINE + 1];
    let mut length = 0;
    let mut chunk = [0u8; 64];
    loop {
        let count = read(&mut chunk);
        for &byte in &chunk[..count] {
            if byte == b'\n' {
                act(&line[..length]);
                length = 0;
            } else if length < line.len() {
                line[length] = byte;
                length += 1;
            }
        }
    }
}

/// Installs the console and starts its thread on core 0; `console: ready` once.
pub fn start() -> Result<(), String> {
    install()?;
    ThreadSpawnConfiguration {
        name: Some(c"console"),
        stack_size: CONSOLE_STACK_BYTES,
        pin_to_core: Some(Core::Core0),
        ..Default::default()
    }
    .set()
    .map_err(|e| format!("console: cannot configure its thread: {e}"))?;
    let spawned = std::thread::Builder::new()
        .name("console".into())
        .stack_size(CONSOLE_STACK_BYTES)
        .spawn(console_loop);
    let _ = ThreadSpawnConfiguration::default().set();
    spawned.map_err(|e| format!("console: cannot start its thread: {e}"))?;
    log::info!("console: ready");
    Ok(())
}
