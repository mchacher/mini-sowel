//! The saving of the home: the single writer, which decides when (`WritePolicy` for the devices;
//! at once for zones, equipments and recipes) and writes through the `Storage`, taking the
//! snapshots under the lock and writing them after the lock is released.

use std::sync::Mutex;

use super::storage::{DEVICES_FILE, EQUIPMENTS_FILE, RECIPES_FILE, Storage, ZONES_FILE};
use crate::devices::{WritePolicy, lock};
use crate::home::{Home, ListCursor};

/// A zones, equipments or recipes file that could not be written is tried again after this, in
/// milliseconds (never in a tight loop).
pub const RETRY_MS: u64 = 60 * 1000;

/// The single writer of the engine's files.
pub struct Saver<S: Storage> {
    store: S,
    policy: WritePolicy,
    /// `pair` writes the devices only: zones and equipments are written by `--serve`.
    devices_only: bool,
    /// Not before this time are the zones and equipments tried again after a failure.
    retry_at: u64,
    /// Where a write failure is shown; the log until the caller gives its own.
    sink: Box<dyn FnMut(&str)>,
}

impl<S: Storage> Saver<S> {
    pub fn new(store: S, now: u64) -> Self {
        Saver {
            store,
            policy: WritePolicy::new(now),
            devices_only: false,
            retry_at: 0,
            sink: Box::new(|message| log::warn!("{message}")),
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

    /// Writes what is due. A failure is shown and the write is tried again later, except on stop
    /// (`stopping`), where it is the error. Each file is written chunk by chunk from a cursor: the
    /// home is taken for one chunk, then let go, then the chunk is written (memory ceiling, spec
    /// 010 R7.15).
    pub fn tick(&mut self, home: &Mutex<Home>, now: u64, stopping: bool) -> Result<(), String> {
        let devices = self.policy.due_stream(&mut lock(home), now, stopping);
        if let Some(cursor) = devices
            && let Err(message) = self.write(home, cursor, DEVICES_FILE)
        {
            if stopping {
                return Err(message);
            }
            (self.sink)(&message);
            self.policy.failed(&mut lock(home).devices, now);
        }
        if self.devices_only || !(stopping || now >= self.retry_at) {
            return Ok(());
        }
        let zones = lock(home).take_zones_cursor();
        if let Some(cursor) = zones
            && let Err(message) = self.write(home, cursor, ZONES_FILE)
        {
            if stopping {
                return Err(message);
            }
            (self.sink)(&message);
            lock(home).mark_zones_changed();
            self.retry_at = now + RETRY_MS;
        }
        let equipments = lock(home).take_equipments_cursor();
        if let Some(cursor) = equipments
            && let Err(message) = self.write(home, cursor, EQUIPMENTS_FILE)
        {
            if stopping {
                return Err(message);
            }
            (self.sink)(&message);
            lock(home).mark_equipments_changed();
            self.retry_at = now + RETRY_MS;
        }
        let recipes = lock(home).take_recipes_cursor();
        if let Some(cursor) = recipes
            && let Err(message) = self.write(home, cursor, RECIPES_FILE)
        {
            if stopping {
                return Err(message);
            }
            (self.sink)(&message);
            lock(home).mark_recipes_changed();
            self.retry_at = now + RETRY_MS;
        }
        Ok(())
    }

    fn write(&self, home: &Mutex<Home>, mut cursor: ListCursor, name: &str) -> Result<(), String> {
        self.store
            .write(name, &mut || cursor.next_chunk(&lock(home)))
    }
}
