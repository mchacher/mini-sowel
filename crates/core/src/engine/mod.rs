//! The engine's loop, shared by the Mac and the ESP32: poll the radio, apply activity, routes and
//! measures, recompute, tick, run the recipes, run the orders, save what is due. Radio, storage,
//! clocks and random source reach it as a transport, a `Storage` and closures; what it shows goes
//! to a sink, never to the terminal.

mod saver;
mod storage;

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub use saver::{
    DEFINITION_DELAY_MS, FLUSH_ASKED_LINE, HOLD_LINE, RETRY_MS, SaveControl, Saver, flushed_line,
};
pub use storage::{
    AUTH_FILE, DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, SCRIPT_EXTENSION, SCRIPTS_DIR,
    SETTINGS_FILE, Storage, WriteCounter, ZONES_FILE, complete_name, flash_line, load_auth,
    load_devices, load_equipments, load_home, load_recipes, load_settings, load_zones,
    recover_interrupted_save, script_file, temporary_name,
};

use crate::devices::lock;
use crate::home::{Home, Phase};
use crate::orders::{Clock, OrderRunner};
use crate::radio::{PairingEvent, Radio, RadioError, RadioTransport, cut_short_line};

pub use crate::home::CLOSE_GRACE_MS;

/// The two clocks the engine runs on.
pub struct Clocks {
    /// Milliseconds since 1970: the timestamps of data. It can jump.
    pub now: Clock,
    /// Milliseconds on a monotonic clock: the recipes' timers and the orders' deadlines.
    pub monotonic: Clock,
}

/// Why the loop ended: the radio failed (a firmware can reset it and run again) or anything else
/// did (storage, the home's data), which a reset of the radio would not mend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    Radio(RadioError),
    Other(String),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Radio(e) => write!(f, "radio: {e}"),
            RunError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<String> for RunError {
    fn from(message: String) -> Self {
        RunError::Other(message)
    }
}

impl From<RadioError> for RunError {
    fn from(e: RadioError) -> Self {
        RunError::Radio(e)
    }
}

impl From<RunError> for String {
    fn from(e: RunError) -> Self {
        e.to_string()
    }
}

/// Runs the loop until `stop` is set; `Ok(true)` then. The radio's network is started and the
/// radio only listens: the caller did it, and shows what it wants of it. `fill` is the random
/// source. With `verbose`, the lines of the measures received go to `sink`, one call per line; the
/// lines of pairing (`pairing: open for 30 s`, `joined:`, `device:`, `reporting:`...) always do.
/// A radio or storage error ends the loop (`RunError::Radio` or `RunError::Other`). The files due are written each turn, and by the caller
/// on stop (`Saver::tick` with `stopping`).
///
/// The pairing window (spec 017) lives in the home: the API records a request, this loop takes
/// it between two polls, opens joining on the radio, follows the devices that join until the time
/// is over and their setups are done (at most the window's grace later), then only listens again.
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
) -> Result<bool, RunError> {
    // The orders of the API (and of the core) wait in this queue; the radio loop is the only one
    // that sends them.
    let orders = lock(home).orders().clone();
    orders.set_clock(clocks.monotonic.clone());
    lock(home).pairing_mut().set_clock(clocks.monotonic.clone());
    let mut runner = OrderRunner::new();
    while !stop.load(Ordering::SeqCst) {
        // The watchdog of the box's engine thread (spec 036 R8): fed at each turn.
        saver.feed();
        pairing_step(radio, home, clocks, sink)?;
        let events = radio.poll_pairing()?;
        let activity = radio.take_activity();
        let routes = radio.take_routes();
        let now = (clocks.now)();
        let busy = radio.busy_devices();
        let pings = {
            let mut guard = lock(home);
            // The clock first: a date taken while it was not set moves before anything reads it.
            guard.observe_wall(now);
            // The announces first: a device's power source is known when it is added.
            for item in &activity {
                guard.devices.apply_activity(item, now);
            }
            for event in &events {
                match event {
                    PairingEvent::Measures(measures) => guard.measures(measures, now, fill)?,
                    other => {
                        sink(&other.line());
                        for note in guard.devices.apply(other, now, fill)? {
                            sink(&note);
                        }
                    }
                }
            }
            // After the events: a device is in the store once it was identified.
            for update in &routes {
                guard.devices.set_route(update);
            }
            guard.pairing_mut().set_busy(&busy);
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
            guard.devices.due_pings(now)
        };
        // A quiet mains device is pinged before it is called offline; the answer comes through
        // the polls as any message. Best effort: a ping that fails is logged, the loop goes on
        // (a dead radio link is caught by the next poll).
        for target in pings {
            match radio.ping(target) {
                Ok(true) => {}
                Ok(false) => log::debug!("ping: no address for a device"),
                Err(e) => log::warn!("ping: {e}"),
            }
        }
        // After the measures: the answer of an order (and the state it makes the device read
        // back) comes through the same polls.
        runner.step(radio, &orders, (clocks.monotonic)());
        if verbose {
            for event in &events {
                if let PairingEvent::Measures(measures) = event {
                    for line in measures.lines() {
                        sink(&line);
                    }
                }
            }
        }
        saver.tick(home, now, false)?;
    }
    // Joining is never left open: closed on stop too (best effort).
    let open = matches!(lock(home).pairing().phase(), Phase::Open { .. });
    if open {
        let _ = radio.close_joining();
        sink("pairing: closed");
        lock(home).pairing_mut().finished();
    }
    Ok(true)
}

/// One turn of the pairing window: the request of the API, the end of the time, the end of the
/// setups that follow it. The radio is called outside the home's lock.
fn pairing_step<T: RadioTransport, C: FnMut() -> u64>(
    radio: &mut Radio<T, C>,
    home: &Mutex<Home>,
    clocks: &Clocks,
    sink: &mut dyn FnMut(&str),
) -> Result<(), RunError> {
    let now = (clocks.monotonic)();
    let (request, phase, grace) = {
        let mut guard = lock(home);
        let window = guard.pairing_mut();
        (window.take_request(), window.phase(), window.grace_ms())
    };
    let mut phase = phase;
    match request {
        // Closing what is not open does nothing; an open window ends like any other.
        Some(0) => {
            if matches!(phase, Phase::Open { .. }) {
                end_window(radio, home, now, grace, sink);
                phase = lock(home).pairing().phase();
            }
        }
        Some(seconds) => {
            // A window opening lets go of the devices whose pairing finished earlier, and keeps
            // the ones still being set up.
            radio.forget_finished();
            radio.set_following(true);
            match radio.permit_join(seconds) {
                Ok(()) => {
                    lock(home)
                        .pairing_mut()
                        .opened(now + u64::from(seconds) * 1000);
                    sink(&format!("pairing: open for {seconds} s"));
                }
                Err(error @ RadioError::Refused { .. }) => {
                    let reason = error.to_string();
                    sink(&format!("pairing: failed: {reason}"));
                    let mut guard = lock(home);
                    guard.pairing_mut().refused(&reason);
                    if radio.pairing_busy() {
                        guard.pairing_mut().ended(now + grace);
                    } else {
                        radio.set_following(false);
                    }
                }
                Err(other) => return Err(other.into()),
            }
            phase = lock(home).pairing().phase();
        }
        None => {}
    }
    match phase {
        Phase::Open { ends_at } if now >= ends_at => end_window(radio, home, now, grace, sink),
        Phase::Finishing { until } => {
            let busy = radio.pairing_busy();
            if !busy || now >= until {
                // The time cap cut the setups in progress short: say which.
                for eui64 in radio.setups_in_progress() {
                    sink(&cut_short_line(&eui64));
                }
                radio.abandon_setups();
                radio.set_following(false);
                lock(home).pairing_mut().finished();
            }
        }
        _ => {}
    }
    Ok(())
}

/// Closes joining (a failure is only a warning: the radio closes it by itself when its time is
/// over) and leaves the devices of the window time to finish.
fn end_window<T: RadioTransport, C: FnMut() -> u64>(
    radio: &mut Radio<T, C>,
    home: &Mutex<Home>,
    now: u64,
    grace: u64,
    sink: &mut dyn FnMut(&str),
) {
    if let Err(error) = radio.close_joining() {
        log::warn!("pairing: closing joining failed: {error}");
    }
    sink("pairing: closed");
    lock(home).pairing_mut().ended(now + grace);
}
