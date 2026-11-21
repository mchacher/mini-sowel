//! The Sowel core of mini-Sowel. It never touches hardware: the host program (Mac) and the
//! firmware (ESP32) give it their adapters.

/// The tool's name, as shown on screen and in the logs.
pub const NAME: &str = "mini-sowel";

/// The version of the core, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The line every program of mini-Sowel prints when it starts: name and version.
pub fn banner() -> String {
    format!("{NAME} {VERSION}")
}

mod memory;
pub use memory::{MemoryStats, PsramStats, memory_line};

pub mod radio;

pub mod api;
pub mod devices;
