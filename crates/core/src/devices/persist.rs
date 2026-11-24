//! When the devices are written to storage, so the box's flash lasts: a change of definitions at
//! the saver's next write (after its definition delay when one is set, spec 025), values and
//! `lastSeen` every 10 minutes when they changed, and on stop or a flush. The storage is an
//! adapter; only one thread (the radio loop) writes.

use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize, Serializer};

use super::model::{
    Device, DeviceData, DeviceOrder, DeviceWithDetails, Id, OrderList, Route, iso_option,
};
use super::registry::Devices;
use crate::home::{Home, ListCursor};

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

/// Locks the devices (or the home that holds them); a lock poisoned by a thread that panicked
/// is recovered, not unwrapped (they are plain data, a half-done change cannot break them).
pub fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(PoisonError::into_inner)
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

    /// The values' interval starts again from `now` (the clock was just set: the time it started
    /// from is meaningless).
    pub fn restart(&mut self, now: u64) {
        self.last_write = now;
    }

    /// True when a write is due now: a change of definitions at once, changed values after the
    /// interval since the last write, or when `stopping`.
    fn is_due(&self, devices: &Devices, now: u64, stopping: bool) -> bool {
        let (definitions, values) = devices.changes();
        let values_due =
            values && (stopping || now.saturating_sub(self.last_write) >= VALUE_WRITE_INTERVAL_MS);
        definitions || values_due
    }

    /// Like `due`, but the file is written chunk by chunk from a cursor (nothing is ever built
    /// whole), the flags being cleared at once: a change made while it is written is due again.
    /// `forced` (a stop or a flush) makes changed values due before their interval.
    pub fn due_stream(&mut self, home: &mut Home, now: u64, forced: bool) -> Option<ListCursor> {
        if !self.is_due(&home.devices, now, forced) {
            return None;
        }
        home.devices.clear_changes();
        self.last_write = now;
        Some(ListCursor::saved_devices(home))
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

/// One device as `devices.json` keeps it (with the `lastChanged` of its data).
pub fn stored_item(item: &DeviceWithDetails) -> impl Serialize + '_ {
    StoredOut {
        device: &item.device,
        data: StoredDataList(item.device.id, &item.data),
        orders: OrderList(item.device.id, &item.orders),
        route: item.route,
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

/// The devices as they are written to `devices.json`: Sowel's `DeviceWithDetails`, and for each
/// data its `lastChanged` (kept between runs, but not part of Sowel's `DeviceData`, so not in the
/// device API).
pub(crate) struct Stored<'a>(pub &'a [DeviceWithDetails]);

impl Serialize for Stored<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(|item| StoredOut {
            device: &item.device,
            data: StoredDataList(item.device.id, &item.data),
            orders: OrderList(item.device.id, &item.orders),
            route: item.route,
        }))
    }
}

#[derive(Serialize)]
struct StoredOut<'a> {
    #[serde(flatten)]
    device: &'a Device,
    data: StoredDataList<'a>,
    orders: OrderList<'a>,
    #[serde(flatten)]
    route: Route,
}

/// The route as `devices.json` writes it: `nwk` and `endpoint`, left out while unknown.
impl Serialize for Route {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        if let Some(nwk) = self.nwk {
            map.serialize_entry("nwk", &nwk)?;
        }
        if let Some(endpoint) = self.endpoint {
            map.serialize_entry("endpoint", &endpoint)?;
        }
        map.end()
    }
}

struct StoredDataList<'a>(Id, &'a [DeviceData]);

impl Serialize for StoredDataList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.1.iter().map(|data| StoredDataOut {
            device_id: self.0,
            data,
            last_changed: data.last_changed,
        }))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredDataOut<'a> {
    device_id: Id,
    #[serde(flatten)]
    data: &'a DeviceData,
    #[serde(with = "iso_option")]
    last_changed: Option<u64>,
}

#[derive(Deserialize)]
struct StoredIn {
    #[serde(flatten)]
    device: Device,
    data: Vec<StoredDataIn>,
    orders: Vec<DeviceOrder>,
    /// Missing in a file written before spec 011.
    #[serde(default)]
    nwk: Option<u16>,
    #[serde(default)]
    endpoint: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredDataIn {
    #[serde(flatten)]
    data: DeviceData,
    /// Missing in a file written before spec 010: `null`.
    #[serde(default, with = "iso_option")]
    last_changed: Option<u64>,
}

/// The devices read back from the JSON `Devices::to_json` wrote.
pub fn parse_list(json: &str) -> Result<Vec<DeviceWithDetails>, String> {
    let list: Vec<StoredIn> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(list
        .into_iter()
        .map(|item| DeviceWithDetails {
            device: item.device,
            data: item
                .data
                .into_iter()
                .map(|d| DeviceData {
                    last_changed: d.last_changed,
                    ..d.data
                })
                .collect(),
            orders: item.orders,
            route: Route {
                nwk: item.nwk,
                endpoint: item.endpoint,
            },
        })
        .collect())
}
