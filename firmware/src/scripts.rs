//! Script recipes on the firmware (feature `scripts`, spec 041, 042): no script is built into any
//! image; the installed files of `scripts/` are loaded when the engine starts (or when the safe mode
//! starts, where each is shown refused), each logging `recipes: script <id> <version> installed
//! (<KB> KB of source, <KB> KB in its runtime)`; and the `scripts:` lines printed with each
//! `heap:` line of the radio image (R6.3). The runtimes live in PSRAM (`script_alloc.rs`).

#[cfg(feature = "radio")]
use std::sync::Mutex;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use solite_core::home::Home;
use solite_core::recipes::ScriptClock;
use solite_core::recipes::script::ScriptRuntimes;
#[cfg(feature = "sim")]
use solite_core::recipes::script::ScriptSummary;
use solite_core::scripts_lines;

use crate::script_alloc::EspRuntimes;

/// The recipe key of this build (spec 042 R1.5): 64 hexadecimal characters, or absent.
#[cfg(feature = "radio")]
const RECIPE_PUBLIC_KEY: Option<&str> = option_env!("SOLITE_RECIPE_PUBLIC_KEY");

/// The monotonic clock of the scripts' time limits, in milliseconds since its first reading
/// (taken at start-up): the engine's loads and the recipe jobs' share it.
pub fn monotonic_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    u64::try_from(START.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The clock the script runtimes are given.
pub fn clock() -> ScriptClock {
    Arc::new(monotonic_ms)
}

/// The stack of the `script` job thread: the JavaScript stack's 16 KB (`SCRIPT_STACK_BYTES`), the
/// C frames of QuickJS and of the engine around it, and the router's frames above. The QEMU image
/// measures it (`script_phase.rs`): its line must show at least 2 KB unused.
pub const SCRIPT_THREAD_STACK_BYTES: usize = 32 * 1024;

/// The runtimes every script recipe of the firmware is made from.
pub fn runtimes() -> Arc<dyn ScriptRuntimes> {
    Arc::new(EspRuntimes)
}

/// Loads one script (the QEMU image's phase); logs its line, or why it was refused (a script that does not load is not the
/// firmware's failure: the other recipes run).
#[cfg(feature = "sim")]
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

/// Gives the home the recipe key and the memory probe, then loads the installed files of
/// `scripts/` as the start does (spec 042 R3.7): in the order of their ids, each logged, before the
/// instances start. In the safe mode each is shown refused (`Safe mode: not loaded`). Reads the
/// storage figures last.
#[cfg(feature = "radio")]
pub fn load_installed(home: &mut Home) {
    let (installs, line) = solite_core::recipes::install::RecipeInstalls::from_key_text(
        RECIPE_PUBLIC_KEY,
        Arc::new(crate::memory::EspProbe),
    );
    log::info!("{line}");
    home.set_recipe_installs(installs);
    let clock = clock();
    for line in home.load_installed_scripts(&crate::storage::FatStorage::new(), &runtimes(), &clock)
    {
        log::info!("{line}");
    }
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
