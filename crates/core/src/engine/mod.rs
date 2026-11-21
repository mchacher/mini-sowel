//! The engine's loop, shared by the Mac and the ESP32: poll the radio, apply activity, routes and
//! measures, recompute, tick, run the recipes, run the orders, save what is due. Radio, storage,
//! clocks and random source reach it as a transport, a `Storage` and closures; what it shows goes
//! to a sink, never to the terminal.

mod saver;
mod storage;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub use saver::{RETRY_MS, Saver};
pub use storage::{
    DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, Storage, ZONES_FILE, complete_name, load_devices,
    load_equipments, load_home, load_recipes, load_zones, recover_interrupted_save, temporary_name,
};

use crate::devices::lock;
use crate::home::Home;
use crate::orders::{Clock, OrderRunner};
use crate::radio::{Radio, RadioTransport};

/// The two clocks the engine runs on.
pub struct Clocks {
    /// Milliseconds since 1970: the timestamps of data. It can jump.
    pub now: Clock,
    /// Milliseconds on a monotonic clock: the recipes' timers and the orders' deadlines.
    pub monotonic: Clock,
}

/// Runs the loop until `stop` is set; `Ok(true)` then. The radio is identified and listening: the
/// caller did it, and shows what it wants of it. `fill` is the random source. With `verbose`, the
/// lines of the measures received go to `sink`, one call per line. A radio or storage error ends
/// the loop. The files due are written each turn, and by the caller on stop (`Saver::tick` with
/// `stopping`).
#[allow(clippy::too_many_arguments)]
pub fn run<T: RadioTransport, C: FnMut() -> u64, S: Storage>(
    radio: &mut Radio<T, C>,
    home: &Mutex<Home>,
    saver: &mut Saver<S>,
    clocks: &Clocks,
    stop: &AtomicBool,
    fill: &mut dyn FnMut(&mut [u8]) -> Result<(), String>,
    verbose: bool,
    sink: &mut dyn FnMut(&str),
) -> Result<bool, String> {
    // The orders of the API (and of the core) wait in this queue; the radio loop is the only one
    // that sends them.
    let orders = lock(home).orders().clone();
    orders.set_clock(clocks.monotonic.clone());
    let mut runner = OrderRunner::new();
    while !stop.load(Ordering::SeqCst) {
        let events = radio.poll().map_err(|e| format!("radio: {e}"))?;
        let activity = radio.take_activity();
        let routes = radio.take_routes();
        let now = (clocks.now)();
        {
            let mut guard = lock(home);
            for item in &activity {
                guard.devices.apply_activity(item, now);
            }
            for update in &routes {
                guard.devices.set_route(update);
            }
            for event in &events {
                guard.measures(event, now, fill)?;
            }
            let status_changed = guard.devices.refresh_status(now);
            // The zone aggregation (and the motion times) after each batch of measures, and
            // when silence alone turned a device offline.
            if status_changed || !events.is_empty() || !activity.is_empty() {
                guard.recompute(now);
            }
            // What goes stale with time alone is seen at least once a minute.
            guard.tick(now);
            // The recipes get the events of this batch (and of the API's edits since the last
            // turn), then the timers that are due fire. Their orders join the queue below.
            guard.run_recipes((clocks.monotonic)(), now);
        }
        // After the measures: the answer of an order (and the state it makes the device read
        // back) comes through the same polls.
        runner.step(radio, &orders, (clocks.monotonic)());
        if verbose {
            for event in &events {
                for line in event.lines() {
                    sink(&line);
                }
            }
        }
        saver.tick(home, now, false)?;
    }
    Ok(true)
}
