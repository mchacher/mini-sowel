//! Script recipes on the firmware (feature `scripts`, spec 041): the scripts of `recipes/scripts/`
//! built into the image (`build.rs` lists them, `SCRIPT_FILES`) and loaded when the engine starts,
//! each logging `recipes: script <id> <version> from built in (<KB> KB of source, <KB> KB in its
//! runtime)`; and the `scripts:` lines printed with each `heap:` line of the radio image (R6.3).
//! The runtimes live in PSRAM (`script_alloc.rs`).

use std::sync::Arc;
#[cfg(feature = "radio")]
use std::sync::{Mutex, OnceLock};

use solite_core::home::Home;
use solite_core::recipes::ScriptClock;
use solite_core::recipes::script::{ScriptRuntimes, ScriptSummary};
use solite_core::scripts_lines;

use crate::script_alloc::EspRuntimes;

include!(concat!(env!("OUT_DIR"), "/script_files.rs"));

/// The stack of the `script` job thread: the JavaScript stack's 16 KB (`SCRIPT_STACK_BYTES`), the
/// C frames of QuickJS and of the engine around it, and the router's frames above. The QEMU image
/// measures it (`script_phase.rs`): its line must show at least 2 KB unused.
pub const SCRIPT_THREAD_STACK_BYTES: usize = 32 * 1024;

/// The runtimes every script recipe of the firmware is made from.
pub fn runtimes() -> Arc<dyn ScriptRuntimes> {
    Arc::new(EspRuntimes)
}

/// Loads one script; logs its line, or why it was refused (a script that does not load is not the
/// firmware's failure: the other recipes run).
pub fn load_one(
    home: &mut Home,
    name: &str,
    source: &str,
    runtimes: &Arc<dyn ScriptRuntimes>,
    clock: &ScriptClock,
) -> Option<ScriptSummary> {
    match home.load_script_recipe(source, Arc::clone(runtimes), Arc::clone(clock)) {
        Ok(summary) => {
            log::info!(
                "recipes: script {} {} from built in ({:.1} KB of source, {:.1} KB in its runtime)",
                summary.id,
                summary.version.as_deref().unwrap_or("-"),
                summary.source_bytes as f64 / 1024.0,
                summary.runtime_bytes as f64 / 1024.0,
            );
            Some(summary)
        }
        Err(reason) => {
            log::error!("recipes: script {name}: {reason}");
            None
        }
    }
}

/// Loads every script built into the image, before the instances start.
#[cfg(feature = "radio")]
pub fn load_built_in(home: &mut Home, clock: &ScriptClock) {
    let runtimes = runtimes();
    for (name, source) in SCRIPT_FILES {
        load_one(home, name, source, &runtimes, clock);
    }
}

/// The source of a script built into the image, by its file name.
#[cfg(feature = "sim")]
pub fn built_in(name: &str) -> Option<&'static str> {
    SCRIPT_FILES
        .iter()
        .find(|(file, _)| *file == name)
        .map(|(_, source)| *source)
}

#[cfg(feature = "radio")]
static WATCHED: OnceLock<Arc<Mutex<Home>>> = OnceLock::new();

/// The home whose script recipes the `scripts:` lines report.
#[cfg(feature = "radio")]
pub fn watch_home(home: &Arc<Mutex<Home>>) {
    let _ = WATCHED.set(Arc::clone(home));
}

/// The `scripts:` lines, one per loaded script recipe. Nothing when the home is busy at this
/// moment (the line comes again with the next `heap:` line): the radio loop that logs them never
/// waits for the home's lock.
#[cfg(feature = "radio")]
pub fn lines() -> Vec<String> {
    let Some(home) = WATCHED.get() else {
        return Vec::new();
    };
    match home.try_lock() {
        Ok(guard) => scripts_lines(&guard.script_stats()),
        Err(_) => Vec::new(),
    }
}

/// The `scripts:` lines of a home the caller holds.
#[cfg(feature = "sim")]
pub fn lines_of(home: &Home) -> Vec<String> {
    scripts_lines(&home.script_stats())
}
