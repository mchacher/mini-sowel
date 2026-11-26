//! The Sowel core of Solite. It never touches hardware: the host program (Mac) and the
//! firmware (ESP32) give it their adapters.

/// The tool's name, as shown on screen and in the logs.
pub const NAME: &str = "solite";

/// The version of the core, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The line every program of Solite prints when it starts: name and version.
pub fn banner() -> String {
    format!("{NAME} {VERSION}")
}

mod memory;
pub use memory::{
    EngineStats, HeapStats, MemoryStats, PsramStats, ScriptPhase, engine_line, heap_line,
    memory_line, script_phase_line, scripts_lines,
};

pub mod radio;

pub mod api;
pub mod auth;
pub mod backup;
pub mod devices;
pub mod engine;
pub mod equipments;
pub mod failure;
pub mod home;
pub mod live;
pub mod network;
pub mod orders;
pub mod recipes;
pub mod settings;
pub mod supervisor;
pub mod update;
pub mod zones;
