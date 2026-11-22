//! Solite on the ESP32: starts ESP-IDF, writes its banner and its free memory to the serial log,
//! mounts its data partition and reads its network setting. The image of the box starts no radio;
//! the image QEMU runs (feature `sim`) runs the engine on the simulated radio; the radio image
//! (feature `radio`) drives the EFR32 over the UART and runs the engine on it.

#[cfg(feature = "sim")]
mod measure;
mod memory;
#[cfg(feature = "radio")]
mod radio_engine;
#[cfg(feature = "radio")]
mod radio_uart;
#[cfg(feature = "sim")]
mod simulated;
mod storage;

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

/// Reads the network setting and logs the choice; no link is started.
fn log_network() {
    let text = match storage::FatStorage::new().read(NETWORK_FILE) {
        Ok(text) => text,
        Err(e) => {
            log::warn!("{e}");
            None
        }
    };
    // No cable detection yet: the choice is made as if none is plugged.
    for line in solite_core::network::boot_lines(text.as_deref(), false) {
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

#[cfg(feature = "radio")]
fn run_engine() {
    let spawned = radio_engine::spawn();
    let result = match spawned {
        Ok(handle) => handle
            .join()
            .map_err(|_| "engine: the thread panicked".to_string()),
        Err(e) => Err(format!("engine: cannot start its thread: {e}")),
    };
    if let Err(e) = result {
        log::error!("{e}");
    }
}

#[cfg(not(any(feature = "sim", feature = "radio")))]
fn run_engine() {
    log::info!("radio: none");
}

fn main() {
    // Links the ESP-IDF runtime patches; must be the first call.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("{}", solite_core::banner());
    log::info!("{}", solite_core::memory_line(&memory::read()));
    match mount_storage() {
        Ok(()) => {
            log_network();
            run_engine();
        }
        Err(e) => log::error!("{e}"),
    }
}
