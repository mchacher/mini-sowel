//! mini-Sowel on the ESP32: starts ESP-IDF, then writes its banner to the serial log.

fn main() {
    // Links the ESP-IDF runtime patches; must be the first call.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("{}", mini_sowel_core::banner());
}
