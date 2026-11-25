//! Solite on the ESP32: starts ESP-IDF, writes its banner and its free memory to the serial log,
//! mounts its data partition and reads its network setting. The image of the box starts no radio
//! and no link; the image QEMU runs (feature `sim`) runs the engine on the simulated radio; the
//! radio image (feature `radio`) drives the EFR32 over the UART and runs the engine on it, joins
//! the Wi-Fi network of `/data/wifi.json`, sets its clock by SNTP, serves Sowel's API, `/ws` and
//! the interface on port 80, and takes the Wi-Fi settings on its serial console (spec 023).

mod app_desc;
#[cfg(feature = "radio")]
mod console;
#[cfg(feature = "radio")]
mod http_server;
#[cfg(feature = "radio")]
mod live_ws;
#[cfg(feature = "sim")]
mod measure;
mod memory;
#[cfg(feature = "radio")]
mod ota;
#[cfg(feature = "radio")]
mod radio_engine;
#[cfg(feature = "radio")]
mod radio_uart;
#[cfg(feature = "sim")]
mod simulated;
mod storage;
#[cfg(feature = "radio")]
mod wifi;

/// The interface packed by `build.rs` (spec 022's `crates/ui-embed`): gzip files in the flash.
#[cfg(feature = "radio")]
mod ui_files {
    include!(concat!(env!("OUT_DIR"), "/ui_files.rs"));
}

#[cfg(all(feature = "radio", feature = "sim"))]
compile_error!("radio and sim are two images");

use solite_core::engine::Storage;

/// The network setting's file, in the data partition.
const NETWORK_FILE: &str = "network.json";

/// Mounts the data partition and logs it. Without it, nothing else can run.
fn mount_storage() -> Result<(), String> {
    let mounted = storage::mount()?;
    if let Some(reason) = mounted.formatted {
        log::info!("storage: formatted ({reason})");
    }
    let (used, total) = storage::usage()?;
    log::info!(
        "storage: {}, {} KB used of {} KB",
        storage::BASE,
        used / 1024,
        total / 1024
    );
    Ok(())
}

/// The network setting's text, or `None` (a read error is logged).
fn network_setting() -> Option<String> {
    match storage::FatStorage::new().read(NETWORK_FILE) {
        Ok(text) => text,
        Err(e) => {
            log::warn!("{e}");
            None
        }
    }
}

/// Reads the network setting and logs the choice, for the images without a link: no link is
/// started (the radio image's path starts Wi-Fi, `run_radio`).
#[cfg(not(feature = "radio"))]
fn log_network() {
    // No cable detection: the choice is made as if none is plugged.
    for line in solite_core::network::boot_lines(network_setting().as_deref(), false) {
        log::info!("{line}");
    }
}

#[cfg(feature = "sim")]
fn run_engine() {
    if let Err(e) = simulated::seed() {
        log::error!("{e}");
        return;
    }
    let before = measure::free_total();
    measure::start_local_minimum();
    let spawned = std::thread::Builder::new()
        .name("engine".into())
        .stack_size(simulated::STACK_BYTES)
        .spawn(move || simulated::run(before));
    let result = match spawned {
        Ok(handle) => handle
            .join()
            .map_err(|_| "engine: the thread panicked".to_string()),
        Err(e) => Err(format!("engine: cannot start its thread: {e}")),
    };
    match result {
        Ok(Ok(stats)) => log::info!("{}", solite_core::engine_line(&stats)),
        Ok(Err(e)) | Err(e) => log::error!("{e}"),
    }
    measure::stop_local_minimum();
}

/// The Wi-Fi settings to start the station with, or `None` with the line that says why not
/// (spec 023 R1.1, R1.3).
#[cfg(feature = "radio")]
fn wifi_settings() -> Option<solite_core::network::WifiSettings> {
    use solite_core::network::{loaded_line, no_settings_line, refused_line, wifi_settings};
    let text = match storage::FatStorage::new().read(console::WIFI_FILE) {
        Ok(Some(text)) => text,
        Ok(None) => {
            log::warn!("{}", no_settings_line());
            return None;
        }
        Err(e) => {
            log::warn!("{e}, no link started");
            return None;
        }
    };
    match wifi_settings(&text) {
        Ok(settings) => {
            log::info!("{}", loaded_line(&settings));
            Some(settings)
        }
        Err(refusal) => {
            log::warn!("{}", refused_line(refusal));
            None
        }
    }
}

/// The radio image after the storage: the link chosen, the interface's marker, the console, the
/// Wi-Fi station, the engine; then `main` runs the link's loop (with Wi-Fi) or waits for the
/// engine (without).
#[cfg(feature = "radio")]
fn run_radio() {
    use solite_core::network::{Link, boot_lines_radio, choose, parse};
    use std::sync::atomic::Ordering;

    let text = network_setting();
    // No cable detection in this image: `auto` means Wi-Fi (Ethernet is E3e).
    for line in boot_lines_radio(text.as_deref(), false) {
        log::info!("{line}");
    }
    log::info!("{}", ui_files::UI_MARKER);
    if let Err(e) = console::start() {
        log::error!("{e}");
    }
    // The updates: the key line, the OTA thread; without it, the update route answers 503.
    if let Err(e) = ota::start() {
        log::error!("{e}");
    }
    let settings = match choose(parse(text.as_deref()).0, false) {
        Link::Wifi => wifi_settings(),
        Link::Ethernet => None,
    };
    let (sender, events) = std::sync::mpsc::channel();
    let station = settings.and_then(|settings| {
        let modem = match esp_idf_svc::hal::peripherals::Peripherals::take() {
            Ok(peripherals) => peripherals.modem,
            Err(e) => {
                log::error!("wifi: {e}, no link started");
                return None;
            }
        };
        match wifi::start(modem, &settings, &sender) {
            Ok(station) => Some(station),
            Err(e) => {
                log::error!("{e}, no link started");
                None
            }
        }
    });
    if station.is_none() {
        // Nothing to wait for: the saves are never held without Wi-Fi.
        wifi::CLOCK_SET.store(true, Ordering::SeqCst);
    }
    let handle = match radio_engine::spawn(station.is_some().then(|| sender.clone())) {
        Ok(handle) => handle,
        Err(e) => {
            log::error!("engine: cannot start its thread: {e}");
            return;
        }
    };
    drop(sender);
    if let Some(station) = station {
        wifi::run(station, &events);
    }
    if handle.join().is_err() {
        log::error!("engine: the thread panicked");
    }
}

#[cfg(not(any(feature = "sim", feature = "radio")))]
fn run_engine() {
    log::info!("radio: none");
}

fn main() {
    // Links the ESP-IDF runtime patches; must be the first call.
    esp_idf_svc::sys::link_patches();
    // The log level stays ESP-IDF's default, info: no target is lowered. esp-idf-svc logs the
    // Wi-Fi configuration, password included, at debug level (target `esp_idf_svc::wifi`), so that
    // target must never go below info (spec 023 R1.6).
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("{}", solite_core::banner());
    log::info!("{}", solite_core::memory_line(&memory::read()));
    match mount_storage() {
        #[cfg(not(feature = "radio"))]
        Ok(()) => {
            log_network();
            run_engine();
        }
        #[cfg(feature = "radio")]
        Ok(()) => run_radio(),
        Err(e) => log::error!("{e}"),
    }
}
