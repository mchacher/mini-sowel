//! mini-Sowel on the ESP32: starts ESP-IDF, then writes its banner and its free memory to the
//! serial log.

mod memory;

fn main() {
    // Links the ESP-IDF runtime patches; must be the first call.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("{}", mini_sowel_core::banner());
    log::info!("{}", mini_sowel_core::memory_line(&memory::read()));
}
