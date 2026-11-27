//! The memory of script recipes on the Mac (spec 041, R6.1), through QuickJS's own accounting
//! (`memory_usage`, what its limit counts: the allocator's usable size), on its own line, apart
//! from the engine's counting allocator and its ceilings (`memory_worst_case.rs`). The figures
//! are printed (`script memory: ...`, read with `--nocapture`) and bounded.
//!
//! A loaded `motion-light` with 0, 1, 3 and 8 instances; what a motion event holds right
//! after the call (QuickJS frees by reference counting, so this is its peak but for temporaries
//! inside the call); and the memory held after 10,000 events, which may grow by 4 KB at most.
//!
//! The Mac counts with 64-bit pointers and `malloc_size`'s rounding: the box's figure (32-bit) is
//! measured in QEMU and written in `docs/memory-budget.md`.
//!
//! These tests need the `script-recipes` feature (the host program turns it on, so
//! `cargo test --workspace` runs them).

mod support;

use std::sync::Arc;

use serde_json::json;
use solite_core::devices::Id;
use solite_core::recipes::ScriptClock;
use solite_core::recipes::native::{NativeHost, RecipeName};
use solite_core::recipes::script::{
    NewRuntime, SCRIPT_MEMORY_LIMIT, ScriptRecipe, ScriptRuntimes, SystemRuntimes,
};
use solite_recipe_sdk::{Clock, Event, EventValue, Recipe};
use support::*;

const SCRIPT: &str = include_str!("../../../recipes/scripts/motion-light.js");
const SECOND: u64 = 1000;

/// The system's allocator with a roomy limit: the table is the measure, so the sixteen instances
/// must not meet the Mac's own limit (`SystemRuntimes`, 256 KB) before they are counted.
struct Roomy;

impl ScriptRuntimes for Roomy {
    fn new_runtime(&self) -> Result<NewRuntime, String> {
        SystemRuntimes.new_runtime()
    }

    fn memory_limit(&self) -> usize {
        1024 * 1024
    }
}

struct World {
    rig: Rig,
    g: Garage,
    recipe: ScriptRecipe,
    names: RecipeName,
}

impl World {
    fn new() -> World {
        let mut rig = Rig::new();
        let g = rig.garage();
        let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(Roomy);
        let clock: ScriptClock = Arc::new(|| 0);
        let (recipe, _) = ScriptRecipe::load(SCRIPT, &runtimes, &clock).unwrap();
        World {
            rig,
            g,
            recipe,
            names: RecipeName {
                id: "motion-light".into(),
                name: "Motion Light".into(),
            },
        }
    }

    fn clock(&self) -> Clock {
        Clock {
            mono: self.rig.now,
            wall: self.rig.now,
        }
    }

    fn start(&mut self, handle: u32) {
        let params = json!({"zone": self.g.zone, "lights": [self.g.light], "timeout": "20s"});
        let clock = self.clock();
        let host = NativeHost::new(
            &self.rig.home,
            Id::ROOT_ZONE,
            &self.names,
            clock.mono,
            clock.wall,
        );
        self.recipe.start(&host, handle, &params, clock).unwrap();
        self.rig.sent();
    }

    /// The zone's motion changes in the home, then every instance is told.
    fn motion(&mut self, handles: &[u32], on: bool) {
        self.rig.measure(&motion(PIR, on));
        let event = Event::Zone {
            zone: self.g.zone.clone(),
            motion: on,
            luminosity: None,
        };
        self.event(handles, &event);
    }

    fn relay(&mut self, handles: &[u32], on: bool) {
        self.rig.measure(&state(RELAY, on));
        let event = Event::Equipment {
            equipment: self.g.light.clone(),
            alias: "state".into(),
            value: EventValue::Bool(on),
        };
        self.event(handles, &event);
    }

    fn event(&mut self, handles: &[u32], event: &Event) {
        let clock = self.clock();
        let host = NativeHost::new(
            &self.rig.home,
            Id::ROOT_ZONE,
            &self.names,
            clock.mono,
            clock.wall,
        );
        for handle in handles {
            self.recipe.on_event(&host, *handle, event, clock).unwrap();
        }
        self.rig.sent();
    }

    fn tick(&mut self, handles: &[u32]) {
        let clock = self.clock();
        let host = NativeHost::new(
            &self.rig.home,
            Id::ROOT_ZONE,
            &self.names,
            clock.mono,
            clock.wall,
        );
        for handle in handles {
            self.recipe.tick(&host, *handle, clock).unwrap();
        }
        self.rig.sent();
    }

    fn held(&self) -> usize {
        self.recipe.held_bytes()
    }

    /// QuickJS's whole accounting, on its own line.
    fn line(&self, what: &str) {
        let m = self.recipe.memory_usage();
        println!(
            "script memory: {what}: malloc {} B, used {} B, {} objects, {} atoms ({} B), {} functions ({} B)",
            m.malloc_size,
            m.memory_used_size,
            m.obj_count,
            m.atom_count,
            m.atom_size,
            m.js_func_count,
            m.js_func_size
        );
    }
}

#[test]
fn a_loaded_recipe_with_0_1_3_and_16_instances_holds_what_the_table_says() {
    let mut w = World::new();
    w.line("loaded, 0 instances");
    let mut held = vec![(0, w.held())];
    let mut next = 0u32;
    for count in [1usize, 3, 8] {
        while (next as usize) < count {
            w.start(next);
            next += 1;
        }
        w.line(&format!("{count} instance(s)"));
        held.push((count, w.held()));
    }
    println!("script memory: held by instances: {held:?}");
    // The bound of the box (`SCRIPT_MEMORY_LIMIT`, 224 KB, counted with 32-bit pointers and the
    // firmware's allocator) is checked in QEMU; here the Mac's figures are only printed, with the
    // box's limit beside them.
    println!("script memory: the box's limit is {SCRIPT_MEMORY_LIMIT} B");
    // An instance is a few KB, not a copy of the recipe.
    let per_instance = (held[3].1 - held[1].1) / 7;
    println!("script memory: about {per_instance} B per instance after the first");
    assert!(per_instance < 8 * 1024, "{per_instance} bytes per instance");
    // The recipe's own cost is the larger part: it is paid once, however many instances run.
    assert!(held[3].1 < held[0].1 + 8 * 8 * 1024);
}

#[test]
fn a_motion_event_holds_little_and_gives_it_back() {
    let mut w = World::new();
    let handles = [0u32];
    w.start(0);
    let before = w.held();
    w.motion(&handles, true);
    let after_motion = w.held();
    w.relay(&handles, true);
    w.motion(&handles, false);
    let armed = w.held();
    w.rig.now += 21 * SECOND;
    w.tick(&handles);
    let after_timeout = w.held();
    w.line("after a motion cycle");
    println!(
        "script memory: a motion event: {before} -> {after_motion} B (peak +{} B), a timer armed {armed} B, after the timeout {after_timeout} B",
        after_motion.saturating_sub(before)
    );
    assert!(
        after_motion.saturating_sub(before) < 8 * 1024,
        "{before} -> {after_motion}"
    );
    assert!(after_timeout <= armed + 1024, "{armed} -> {after_timeout}");
}

#[test]
fn the_memory_held_after_10000_events_grows_by_at_most_4_kb() {
    let mut w = World::new();
    let handles = [0u32, 1, 2];
    for handle in handles {
        w.start(handle);
    }
    let cycle = |w: &mut World| {
        w.rig.now += 30 * SECOND;
        w.motion(&handles, true);
        w.relay(&handles, true);
        w.motion(&handles, false);
        w.rig.now += 21 * SECOND;
        w.tick(&handles);
        w.relay(&handles, false);
    };
    // The first cycles fill the instances' logs and state: the base line is taken after them.
    for _ in 0..100 {
        cycle(&mut w);
    }
    let base = w.held();
    let mut peak = base;
    // Five events a cycle for three instances: 2,000 cycles are 10,000 events of the home, each
    // sent to the three instances.
    for round in 0..2000 {
        cycle(&mut w);
        if round % 100 == 0 {
            peak = peak.max(w.held());
        }
    }
    let end = w.held();
    w.line("after 10,000 events");
    println!(
        "script memory: 10,000 events: {base} B after warm-up, {end} B at the end (+{} B), highest seen {peak} B",
        end.saturating_sub(base)
    );
    assert!(
        end <= base + 4 * 1024,
        "{base} -> {end}: grows by more than 4 KB"
    );
}
