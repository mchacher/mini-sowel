//! The saving of the home: the single writer, which decides when (`WritePolicy` for the devices'
//! values; the definitions at the next turn, or after the definition delay when one is set) and
//! writes through the `Storage`, taking the snapshots under the lock and writing them after the
//! lock is released. Saves can be held and flushed (`SaveControl`, spec 025).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::storage::{
    AUTH_FILE, DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, SETTINGS_FILE, Storage, ZONES_FILE,
};
use crate::devices::{WritePolicy, lock};
use crate::home::{Home, ListCursor};

/// A zones, equipments, recipes, settings or auth file that could not be written is tried again after this, in
/// milliseconds (never in a tight loop).
pub const RETRY_MS: u64 = 60 * 1000;

/// A change of definitions (devices, zones, equipments, recipes, settings) is written this long after the
/// saver first sees it unsaved, in milliseconds, so that a burst of changes gives one write
/// (decision D40). Bound: at most this long, plus one turn of the engine's loop, a change of
/// definitions is in RAM only (lost on a power cut); a stop writes at once.
pub const DEFINITION_DELAY_MS: u64 = 2_000;

/// The saves held and flushed from the box's serial console (spec 025 R1): two flags, in RAM only,
/// so a restart starts with saves not held. One `static` of it is shared by the console and the
/// engine's saver.
#[derive(Debug, Default)]
pub struct SaveControl {
    held: AtomicBool,
    flush: AtomicBool,
}

impl SaveControl {
    pub const fn new() -> Self {
        SaveControl {
            held: AtomicBool::new(false),
            flush: AtomicBool::new(false),
        }
    }

    /// Nothing is written from now on (but on stop), until a flush.
    pub fn hold(&self) {
        self.held.store(true, Ordering::SeqCst);
    }

    /// Asks the saver to write, at its next turn, every file with unsaved changes once, then to end
    /// the hold.
    pub fn ask_flush(&self) {
        self.flush.store(true, Ordering::SeqCst);
    }

    pub fn is_held(&self) -> bool {
        self.held.load(Ordering::SeqCst)
    }

    /// The flush asked, taken once; the hold ends with it.
    fn take_flush(&self) -> bool {
        let asked = self.flush.swap(false, Ordering::SeqCst);
        if asked {
            self.held.store(false, Ordering::SeqCst);
        }
        asked
    }
}

/// The console's answer to `saves hold`.
pub const HOLD_LINE: &str = "saves: held, nothing is written until saves flush or a restart";
/// The console's answer to `saves flush`.
pub const FLUSH_ASKED_LINE: &str = "saves: flush asked";

/// The engine's line once a flush is done.
pub fn flushed_line(files: usize) -> String {
    format!("saves: flushed, {files} files written")
}

/// The single writer of the engine's files.
pub struct Saver<S: Storage> {
    store: S,
    policy: WritePolicy,
    /// `pair` writes the devices only: zones, equipments, recipes and settings are written by
    /// `--serve`.
    devices_only: bool,
    /// Not before this time are the zones and equipments tried again after a failure.
    retry_at: u64,
    /// Where a write failure is shown; the log until the caller gives its own.
    sink: Box<dyn FnMut(&str)>,
    /// While it answers `false`, nothing is written but on stop (the box's clock wait, spec 023
    /// R3.2); `None`: always open.
    gate: Option<Box<dyn Fn() -> bool>>,
    /// The hold and flush of the serial console; `None`: never held.
    control: Option<&'static SaveControl>,
    /// The definition delay, in milliseconds; 0: definitions written at the next turn.
    delay_ms: u64,
    /// When the saver first saw definitions unsaved (the wall clock it is given).
    pending_since: Option<u64>,
    /// The gate was closed at the last tick: at its opening the values' interval starts again,
    /// so a clock set from 1970 does not make the values due at once (one write saved per boot).
    gate_was_closed: bool,
}

impl<S: Storage> Saver<S> {
    pub fn new(store: S, now: u64) -> Self {
        Saver {
            store,
            policy: WritePolicy::new(now),
            devices_only: false,
            retry_at: 0,
            sink: Box::new(|message| log::warn!("{message}")),
            gate: None,
            control: None,
            delay_ms: 0,
            pending_since: None,
            gate_was_closed: false,
        }
    }

    /// A saver that writes `devices.json` only.
    pub fn devices_only(store: S, now: u64) -> Self {
        Saver {
            devices_only: true,
            ..Saver::new(store, now)
        }
    }

    /// Shows write failures through `sink` instead of the log.
    #[must_use]
    pub fn with_sink(mut self, sink: impl FnMut(&str) + 'static) -> Self {
        self.sink = Box::new(sink);
        self
    }

    /// Holds the writes while `gate` answers `false`: every file stays due, nothing is shown or
    /// counted as failed and no retry delay starts; the first tick after it answers `true` writes
    /// what is due. A stop writes whatever it answers.
    #[must_use]
    pub fn with_gate(mut self, gate: impl Fn() -> bool + 'static) -> Self {
        self.gate = Some(Box::new(gate));
        self
    }

    /// Follows the console's hold and flush (spec 025 R1).
    #[must_use]
    pub fn with_control(mut self, control: &'static SaveControl) -> Self {
        self.control = Some(control);
        self
    }

    /// Writes a change of definitions `delay_ms` after it is first seen unsaved (spec 025 R3);
    /// 0 gives back the write at the next turn.
    #[must_use]
    pub fn with_definition_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Writes what is due. A failure is shown and the write is tried again later, except on stop
    /// (`stopping`), where it is the error. A stop or a flush is forced: every file with unsaved
    /// changes is written, whatever the hold, the delay and the retry time. Each file is written
    /// chunk by chunk from a cursor: the home is taken for one chunk, then let go, then the chunk
    /// is written (memory ceiling, spec 010 R7.15).
    pub fn tick(&mut self, home: &Mutex<Home>, now: u64, stopping: bool) -> Result<(), String> {
        // Before anything is written: a date taken before the clock was set is moved first.
        lock(home).observe_wall(now);
        if !stopping && self.gate.as_ref().is_some_and(|open| !open()) {
            self.gate_was_closed = true;
            return Ok(());
        }
        if self.gate_was_closed && !stopping {
            self.gate_was_closed = false;
            self.policy.restart(now);
        }
        let flush = !stopping && self.control.is_some_and(SaveControl::take_flush);
        if !stopping && !flush && self.control.is_some_and(SaveControl::is_held) {
            return Ok(());
        }
        let forced = stopping || flush;
        if !forced && !self.definitions_settled(home, now) {
            return Ok(());
        }
        let (written, failed) = self.write_due(home, now, forced, stopping)?;
        // The wait ends with the writes (a change made while they ran waits its own delay). While
        // a failed file waits for its retry the wait stays over, so the retry comes at its own
        // time, never later (and a change meanwhile is written at the next turn).
        if !failed && now >= self.retry_at {
            self.pending_since = None;
        }
        if flush {
            log::info!("{}", flushed_line(written));
        }
        Ok(())
    }

    /// True when a change of definitions is not written yet.
    fn definitions_unsaved(&self, home: &Mutex<Home>) -> bool {
        let home = lock(home);
        let (zones, equipments) = home.changes();
        home.devices.changes().0
            || (!self.devices_only
                && (zones
                    || equipments
                    || home.recipes_changed()
                    || home.settings_changed()
                    || home.auth_changed()))
    }

    /// False while unsaved definitions wait for the delay; true when they may be written (or there
    /// are none). The wait counts from the first tick that sees them, and restarts from `now` when
    /// the clock went back.
    fn definitions_settled(&mut self, home: &Mutex<Home>, now: u64) -> bool {
        if self.delay_ms == 0 || !self.definitions_unsaved(home) {
            return true;
        }
        match self.pending_since {
            Some(since) if now >= since => now - since >= self.delay_ms,
            _ => {
                self.pending_since = Some(now);
                false
            }
        }
    }

    /// Writes the files due; the number written, and whether one failed. `forced`: every file with unsaved changes;
    /// `stopping`: a failure is the error.
    fn write_due(
        &mut self,
        home: &Mutex<Home>,
        now: u64,
        forced: bool,
        stopping: bool,
    ) -> Result<(usize, bool), String> {
        let (mut written, mut failed) = (0, false);
        let devices = self.policy.due_stream(&mut lock(home), now, forced);
        if let Some(cursor) = devices {
            match self.write(home, cursor, DEVICES_FILE) {
                Ok(()) => written += 1,
                Err(message) if stopping => return Err(message),
                Err(message) => {
                    failed = true;
                    (self.sink)(&message);
                    self.policy.failed(&mut lock(home).devices, now);
                }
            }
        }
        if self.devices_only || !(forced || now >= self.retry_at) {
            return Ok((written, failed));
        }
        type Take = fn(&mut Home) -> Option<ListCursor>;
        type Mark = fn(&mut Home);
        let files: [(&str, Take, Mark); 5] = [
            (
                ZONES_FILE,
                Home::take_zones_cursor,
                Home::mark_zones_changed,
            ),
            (
                EQUIPMENTS_FILE,
                Home::take_equipments_cursor,
                Home::mark_equipments_changed,
            ),
            (
                RECIPES_FILE,
                Home::take_recipes_cursor,
                Home::mark_recipes_changed,
            ),
            (
                SETTINGS_FILE,
                Home::take_settings_cursor,
                Home::mark_settings_changed,
            ),
            (AUTH_FILE, Home::take_auth_cursor, Home::mark_auth_changed),
        ];
        for (name, take, mark) in files {
            let cursor = take(&mut lock(home));
            let Some(cursor) = cursor else {
                continue;
            };
            match self.write(home, cursor, name) {
                Ok(()) => written += 1,
                Err(message) if stopping => return Err(message),
                Err(message) => {
                    failed = true;
                    (self.sink)(&message);
                    mark(&mut lock(home));
                    self.retry_at = now + RETRY_MS;
                }
            }
        }
        Ok((written, failed))
    }

    fn write(&self, home: &Mutex<Home>, mut cursor: ListCursor, name: &str) -> Result<(), String> {
        self.store
            .write(name, &mut || cursor.next_chunk(&lock(home)))
    }
}
