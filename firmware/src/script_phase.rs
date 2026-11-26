//! The script phase of the image QEMU runs (feature `scripts`, spec 041 R6.2), after the engine's
//! own line (its gate, D27, is untouched: no script is loaded before it). On the realistic home
//! the engine just ran:
//!
//! 1. loads `motion-light-script`, swaps the home's three `motion-light` instances for three
//!    instances of it with the same settings, and plays the scenario again (a motion event);
//! 2. loads a script that recurses as deep as the stack limit allows and calls `get` and `order` at
//!    its deepest (`fixtures/depth-host.js`), and starts an instance of it twice: on this (the
//!    engine) thread, and on a `script` job thread, as the radio image's HTTP server runs a
//!    request that changes something (the router's frames included, `handle_stream`);
//! 3. logs `recipes: script motion-light-script ...` (the memory taken by the phase, the lowest
//!    free internal RAM, the two stacks), and the `scripts:` line the radio image prints.
//!
//! Then, outside that line's window, the same with `MAX_INSTANCES_PER_SCRIPT` instances (8, the most one
//! script recipe holds), to read the script's memory at the bound: `script sizes: ...`.
//!
//! `scripts/qemu.sh` fails when the line is missing, the three instances' peak (the `scripts:`
//! line's) passes 75 % of the script memory limit, the internal RAM's minimum is under 64 KB, or
//! either stack keeps less than 2 KB unused.

use std::cell::Cell;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use solite_core::api::{Reach, Reply, Request, handle_stream};
use solite_core::devices::lock;
use solite_core::engine::{self, Clocks, Saver};
use solite_core::home::Home;
use solite_core::radio::Radio;
use solite_core::script_phase_line;
use solite_core::{
    ScriptPhase,
    recipes::{MAX_INSTANCES_PER_SCRIPT, ScriptClock},
};
use solite_sim::Scenario;

use super::{
    BASE, FatStorage, MEMORY_NAME, RunStorage, SCENARIO, SCENARIO_NAME, SPEED, STACK_BYTES,
    SimTransport, WaitedClock, Watched, fill, measure, memory, wall_ms,
};
use crate::job_thread;
use crate::scripts::{self, SCRIPT_THREAD_STACK_BYTES};

/// QEMU's emulated CPU runs the engine far slower than the box's (the first load of a script, which
/// takes a few ms on the box, passes 100 ms there): the scripts' time limits read a clock running
/// this many times slower, so that the phase measures memory and stacks, which do not depend on the
/// speed. The time limits themselves are tested on the Mac.
const QEMU_SLOWDOWN: u64 = 50;

const MOTION_SCRIPT: &str = "motion-light-script";
const DEPTH_SCRIPT: &str = include_str!("../fixtures/depth-host.js");
/// The realistic home's zone and first light (`fixtures/recipes.json`): the depth script's slots.
const ZONE: &str = "00000191-5a5a-4a5a-9a5a-5a5a5a5a5a5a";
const LIGHT: &str = "000002ab-5a5a-4a5a-9a5a-5a5a5a5a5a5a";

/// Plays the realistic scenario again on the home as it is, as the first run did (the radio is a
/// new simulated one; the saves are run and dropped).
fn replay(home: &Mutex<Home>, clocks: &Clocks, start: Instant) -> Result<(), String> {
    let scenario = Scenario::parse(SCENARIO, SCENARIO_NAME)?;
    let memory_file = format!("{BASE}/{MEMORY_NAME}");
    let stop = Arc::new(AtomicBool::new(false));
    let mut inner = SimTransport::for_scenario(
        &scenario,
        Some(Path::new(&memory_file)),
        SPEED,
        WaitedClock {
            waited_ms: Cell::new(0),
        },
    )?;
    inner.keep_memory_in_ram();
    inner.hold();
    let go = Arc::new(AtomicBool::new(false));
    let transport = Watched {
        inner,
        go: Arc::clone(&go),
        started: false,
        stop: Arc::clone(&stop),
    };
    let mut radio = Radio::new(transport, move || {
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
    });
    radio.identify().map_err(|e| format!("radio: {e}"))?;
    radio.listen().map_err(|e| format!("radio: {e}"))?;
    let mut saver = Saver::new(
        RunStorage {
            inner: FatStorage::new(),
            to_flash: Arc::new(AtomicBool::new(false)),
            dropped: Arc::new(AtomicUsize::new(0)),
        },
        wall_ms(),
    );
    go.store(true, std::sync::atomic::Ordering::SeqCst);
    engine::run(
        &mut radio,
        home,
        &mut saver,
        clocks,
        &stop,
        &mut fill,
        false,
        &mut |_| {},
    )?;
    Ok(())
}

/// The params text of each `motion-light` instance of the home, and their ids, for an instance of
/// the script: the same settings, without `buttons`. The fixture's one button is a temperature
/// sensor (synthetic data): the built-in recipe takes it, the script's own `validate` refuses an
/// equipment with no `action` data (R5.2).
fn motion_light_instances(home: &Mutex<Home>) -> Vec<(String, String)> {
    lock(home)
        .recipes()
        .instances()
        .filter(|i| i.recipe_id == "motion-light")
        .map(|i| {
            let mut params: serde_json::Value = serde_json::from_str(&i.params).unwrap_or_default();
            if let Some(object) = params.as_object_mut() {
                object.remove("buttons");
            }
            (i.id.to_string(), params.to_string())
        })
        .collect()
}

fn create(home: &Mutex<Home>, recipe: &str, params: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(params).map_err(|e| e.to_string())?;
    lock(home)
        .create_recipe_instance(recipe, value, wall_ms(), &mut fill)
        .map(|i| i.id.to_string())
        .map_err(|f| format!("script phase: {recipe}: {} {}", f.status, f.message))
}

fn delete_all(home: &Mutex<Home>, recipe: &str) {
    let ids: Vec<String> = lock(home)
        .recipes()
        .instances()
        .filter(|i| i.recipe_id == recipe)
        .map(|i| i.id.to_string())
        .collect();
    for id in ids {
        if let Err(f) = lock(home).delete_recipe_instance(&id) {
            log::warn!("script phase: delete {id}: {}", f.message);
        }
    }
}

/// What the last instance of `recipe` logged (the depth script says how deep it went).
fn last_log(home: &Mutex<Home>, id: &str) -> String {
    lock(home)
        .recipes()
        .log(id, 1)
        .first()
        .map_or_else(String::new, |entry| entry.message.to_string())
}

/// The counters of `id`: what its runtime holds and the most it held.
fn stats_of(home: &Mutex<Home>, id: &str) -> Option<(usize, usize)> {
    lock(home)
        .script_stats()
        .into_iter()
        .find(|(name, _)| name == id)
        .map(|(_, s)| (s.held, s.peak))
}

pub fn run(home: &Mutex<Home>, clocks: &Clocks, start: Instant) -> Result<(), String> {
    log::info!(
        "script phase: engine stack {} of {STACK_BYTES} bytes unused before it",
        job_thread::stack_unused()
    );
    let runtimes = scripts::runtimes();
    let real = Arc::clone(&clocks.monotonic);
    let clock: ScriptClock = Arc::new(move || real() / QEMU_SLOWDOWN);
    // A window of its own: the monitor of the local minimum starts again.
    measure::stop_local_minimum();
    measure::start_local_minimum();
    let mut lows = measure::Lows::new();
    let before = memory::read();
    let psram_before = before.psram.map_or(0, |p| p.free);

    let file = format!("{MOTION_SCRIPT}.js");
    let source =
        scripts::built_in(&file).ok_or("script phase: motion-light-script is not built in")?;
    let began = Instant::now();
    let summary = scripts::load_one(&mut lock(home), &file, source, &runtimes, &clock)
        .ok_or("script phase: motion-light-script did not load")?;
    log::info!(
        "script phase: loaded in {} ms of QEMU's time",
        began.elapsed().as_millis()
    );

    // 1. Three instances with the three `motion-light` instances' settings, the scenario again.
    let originals = motion_light_instances(home);
    if originals.is_empty() {
        return Err("script phase: the home has no motion-light instance".into());
    }
    delete_all(home, "motion-light");
    for (_, params) in &originals {
        create(home, MOTION_SCRIPT, params)?;
    }
    replay(home, clocks, start)?;
    let (held3, peak3) = stats_of(home, MOTION_SCRIPT).ok_or("script phase: no counters")?;
    for line in scripts::lines_of(&lock(home)) {
        log::info!("{line}");
    }

    // 2. The deep recursion, on this thread and on a `script` job thread.
    let depth_params = format!(r#"{{"zone":"{ZONE}","lights":["{LIGHT}"]}}"#);
    scripts::load_one(
        &mut lock(home),
        "depth-host.js",
        DEPTH_SCRIPT,
        &runtimes,
        &clock,
    )
    .ok_or("script phase: depth-host did not load")?;
    let id = create(home, "depth-host", &depth_params)?;
    log::info!("script phase: engine thread, {}", last_log(home, &id));
    let engine_unused = job_thread::stack_unused();
    delete_all(home, "depth-host");
    let body = format!(r#"{{"recipeId":"depth-host","params":{depth_params}}}"#);
    let ran = job_thread::run(c"script", SCRIPT_THREAD_STACK_BYTES, || {
        let mut random = |bytes: &mut [u8]| fill(bytes);
        let request = Request {
            method: "POST",
            path: "/api/v1/recipe-instances",
            host: Some("localhost:80"),
            port: 80,
            reach: Reach::Lan { name: "solite" },
            content_type: Some("application/json"),
            authorization: None,
            content_length: Some(body.len() as u64),
            body: body.as_bytes(),
        };
        let reply = handle_stream(&mut lock(home), &request, wall_ms(), &mut random);
        let status = match reply {
            Reply::Whole(response) => response.status,
            _ => 0,
        };
        (status, job_thread::stack_unused())
    })?;
    let (status, script_unused) = ran.ok_or("script phase: the script thread panicked")?;
    let id = lock(home)
        .recipes()
        .instances()
        .find(|i| i.recipe_id == "depth-host")
        .map(|i| i.id.to_string())
        .unwrap_or_default();
    log::info!(
        "script phase: script thread, POST status {status}, {}",
        last_log(home, &id)
    );
    delete_all(home, "depth-host");

    // 3. The line.
    lows.mark();
    let now = memory::read();
    let phase = ScriptPhase {
        source: summary.source_bytes,
        psram_used: psram_before.saturating_sub(now.psram.map_or(0, |p| p.free)),
        psram_peak: psram_before.saturating_sub(lows.psram),
        internal_used: before.internal_free.saturating_sub(now.internal_free),
        internal_min: lows.internal,
        engine_stack: (engine_unused, STACK_BYTES),
        script_stack: (script_unused, SCRIPT_THREAD_STACK_BYTES),
    };
    log::info!("{}", script_phase_line(MOTION_SCRIPT, &phase));

    // 4. Eight instances, the most one script recipe holds: its memory at the bound.
    delete_all(home, MOTION_SCRIPT);
    for i in 0..MAX_INSTANCES_PER_SCRIPT {
        let (_, params) = &originals[i % originals.len()];
        create(home, MOTION_SCRIPT, params)?;
    }
    replay(home, clocks, start)?;
    let (held_max, peak_max) = stats_of(home, MOTION_SCRIPT).ok_or("script phase: no counters")?;
    log::info!(
        "script sizes: {MOTION_SCRIPT} loaded {} B, 3 instances {held3} B (peak {peak3} B), {MAX_INSTANCES_PER_SCRIPT} instances {held_max} B (peak {peak_max} B)",
        summary.runtime_bytes,
    );
    for line in scripts::lines_of(&lock(home)) {
        log::info!("{line}");
    }
    Ok(())
}
