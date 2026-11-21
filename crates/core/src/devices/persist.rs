//! When the devices are written to storage, so the box's flash lasts: a change of definitions at
//! once, values and `lastSeen` every 10 minutes when they changed, and on stop. The storage is an
//! adapter; only one thread (the radio loop) writes.

use std::sync::{Mutex, MutexGuard, PoisonError};

use super::model::DeviceWithDetails;
use super::registry::Devices;

/// Values and `lastSeen` are written at most this often, in milliseconds (6 writes an hour).
pub const VALUE_WRITE_INTERVAL_MS: u64 = 10 * 60 * 1000;

/// Where the devices are kept between runs: the host's `devices.json`, the box's flash.
pub trait DeviceStore {
    /// All the devices; an empty list when nothing was saved yet. The error says why the
    /// storage cannot be read, and the storage is then never overwritten.
    fn load(&self) -> Result<Vec<DeviceWithDetails>, String>;
    /// Replaces everything saved with this JSON (the list `Devices::to_json` gives).
    fn save(&self, json: &str) -> Result<(), String>;
}

/// Locks the devices; a lock poisoned by a thread that panicked is recovered, not unwrapped
/// (the devices are plain data, a half-done change cannot break them).
pub fn lock(devices: &Mutex<Devices>) -> MutexGuard<'_, Devices> {
    devices.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Decides when to write. The caller takes the JSON under the lock, releases the lock, then
/// writes it, so no I/O ever happens under the lock.
#[derive(Debug)]
pub struct WritePolicy {
    last_write: u64,
}

impl WritePolicy {
    /// `now` is the start time in milliseconds: the first value write is 10 minutes later.
    pub fn new(now: u64) -> Self {
        WritePolicy { last_write: now }
    }

    /// The snapshot to write now, or `None`. A change of definitions is due at once; changed
    /// values are due after the interval since the last write, or when `stopping`.
    pub fn due(&mut self, devices: &mut Devices, now: u64, stopping: bool) -> Option<String> {
        let (definitions, values) = devices.changes();
        let values_due =
            values && (stopping || now.saturating_sub(self.last_write) >= VALUE_WRITE_INTERVAL_MS);
        if !definitions && !values_due {
            return None;
        }
        // No copy of the devices: serialised straight into a buffer of the exact size.
        let json = devices.to_json().ok()?;
        devices.clear_changes();
        self.last_write = now;
        Some(json)
    }

    /// The write failed: what was in it is due again after the interval (never in a tight loop).
    pub fn failed(&mut self, devices: &mut Devices, now: u64) {
        devices.mark_values_changed();
        self.last_write = now;
    }
}

/// Counts the bytes written to it and keeps none.
struct Counter(usize);

impl std::io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The JSON of a value in a buffer of exactly its size: counted first, so the buffer never
/// doubles (a doubling buffer would peak at three times the body on a small box).
pub fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> Option<String> {
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).ok()?;
    let mut bytes = Vec::with_capacity(counter.0);
    serde_json::to_writer(&mut bytes, value).ok()?;
    String::from_utf8(bytes).ok()
}

/// The devices read back from the JSON `Devices::to_json` wrote.
pub fn parse_list(json: &str) -> Result<Vec<DeviceWithDetails>, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}
