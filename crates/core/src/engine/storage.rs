//! Where the engine's files live. The Mac keeps them in a folder, the box in its data partition;
//! the engine reads and writes them by name through this trait and never touches a file itself.

use std::io;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::devices::{Devices, parse_list};
use crate::equipments::{self, StoredEquipment};
use crate::home::Home;
use crate::recipes::RecipeInstance;
use crate::zones::{self, Zone};

/// The devices, as Sowel's `DeviceWithDetails` list.
pub const DEVICES_FILE: &str = "devices.json";
/// The zones.
pub const ZONES_FILE: &str = "zones.json";
/// The equipments with their bindings.
pub const EQUIPMENTS_FILE: &str = "equipments.json";
/// The recipe instances.
pub const RECIPES_FILE: &str = "recipes.json";

/// The writes of a storage to its medium since boot (spec 025 R2): a 32-bit count that stops at
/// its maximum instead of wrapping, so nothing grows with the writes. The firmware keeps one in a
/// `static` and logs `flash_line` after each write.
#[derive(Debug, Default)]
pub struct WriteCounter(AtomicU32);

impl WriteCounter {
    pub const fn new() -> Self {
        WriteCounter(AtomicU32::new(0))
    }

    /// A counter that starts at `count` (to test its bound).
    pub const fn starting_at(count: u32) -> Self {
        WriteCounter(AtomicU32::new(count))
    }

    /// One write more; the count after it.
    pub fn record(&self) -> u32 {
        let mut current = self.0.load(Ordering::SeqCst);
        loop {
            if current == u32::MAX {
                return u32::MAX;
            }
            match self.0.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return current + 1,
                Err(seen) => current = seen,
            }
        }
    }

    pub fn count(&self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// The line logged after each write: `flash: <n> writes since boot`.
pub fn flash_line(count: u32) -> String {
    format!("flash: {count} writes since boot")
}

/// The engine's named files.
pub trait Storage {
    /// The text of a file; a missing file is `None` (first run); one that cannot be read is the
    /// error, a full line naming the file (`devices: <where>: cannot read: <reason>`).
    fn read(&self, name: &str) -> Result<Option<String>, String>;

    /// Replaces a file with what `next` gives, chunk by chunk until it gives `None` (D15): a
    /// partial file is never put in place, and one that existed is kept when `next` fails. The
    /// error is a full line naming the file (`zones: <where>: cannot write: <reason>`).
    fn write(
        &self,
        name: &str,
        next: &mut dyn FnMut() -> io::Result<Option<String>>,
    ) -> Result<(), String>;

    /// Where a file is, for the messages (a path on the Mac).
    fn locate(&self, name: &str) -> String;
}

/// The name a file is written under while its content is being written: `name` and `.tmp`. A
/// file with this name may be cut short, so it is never trusted.
pub fn temporary_name(name: &str) -> String {
    format!("{name}.tmp")
}

/// The name a complete file waits under until it replaces `name`: `name` and `.new`. A save writes
/// `.tmp`, syncs it, renames it to `.new`, deletes the old file, then renames `.new` to `name`.
pub fn complete_name(name: &str) -> String {
    format!("{name}.new")
}

/// Finishes a save that a power cut interrupted. FatFS refuses a rename onto an existing file, so
/// a save deletes the old file, then renames the complete `.new` file. A `.tmp` file is a write
/// that did not end (it may be empty or cut short): it is deleted and never promoted. When `path`
/// is missing and `complete` exists, `complete` is renamed to `path` and a line is logged;
/// `Ok(true)` then. A `complete` file next to an existing `path` is deleted (the existing file is
/// the good one; `Ok(false)`). `P` is a path (`std::path::Path`) or any name the caller's
/// `exists`, `rename` and `remove` understand.
pub fn recover_interrupted_save<P: ?Sized + std::fmt::Debug>(
    path: &P,
    complete: &P,
    temporary: &P,
    exists: impl Fn(&P) -> bool,
    rename: impl Fn(&P, &P) -> io::Result<()>,
    remove: impl Fn(&P) -> io::Result<()>,
) -> io::Result<bool> {
    if exists(temporary) {
        remove(temporary)?;
    }
    if !exists(complete) {
        return Ok(false);
    }
    if exists(path) {
        remove(complete)?;
        return Ok(false);
    }
    rename(complete, path)?;
    log::warn!("{path:?} was missing: recovered from {complete:?} (a save was interrupted)");
    Ok(true)
}

fn cannot_read(
    storage: &dyn Storage,
    what: &str,
    name: &str,
    reason: impl std::fmt::Display,
) -> String {
    format!("{what}: {}: cannot read: {reason}", storage.locate(name))
}

/// The zones; the root alone when there is no file. A file that cannot be read or parsed is an
/// error, and the file is left as it is.
pub fn load_zones(storage: &dyn Storage) -> Result<Vec<Zone>, String> {
    let Some(text) = storage.read(ZONES_FILE)? else {
        return Ok(Vec::new());
    };
    zones::parse_list(&text).map_err(|e| cannot_read(storage, "zones", ZONES_FILE, e))
}

/// The equipments; none when there is no file.
pub fn load_equipments(storage: &dyn Storage) -> Result<Vec<StoredEquipment>, String> {
    let Some(text) = storage.read(EQUIPMENTS_FILE)? else {
        return Ok(Vec::new());
    };
    equipments::parse_list(&text)
        .map_err(|e| cannot_read(storage, "equipments", EQUIPMENTS_FILE, e))
}

/// The recipe instances; none when there is no file.
pub fn load_recipes(storage: &dyn Storage) -> Result<Vec<RecipeInstance>, String> {
    let Some(text) = storage.read(RECIPES_FILE)? else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&text).map_err(|e| cannot_read(storage, "recipes", RECIPES_FILE, e))
}

/// The devices; none when there is no file.
pub fn load_devices(
    storage: &dyn Storage,
) -> Result<Vec<crate::devices::DeviceWithDetails>, String> {
    let Some(text) = storage.read(DEVICES_FILE)? else {
        return Ok(Vec::new());
    };
    parse_list(&text).map_err(|e| cannot_read(storage, "devices", DEVICES_FILE, e))
}

/// The whole home, made consistent; a file that cannot be read stops everything. The recipe
/// instances are loaded, not started: the caller starts them (`Home::start_recipes`).
pub fn load_home(storage: &dyn Storage, now: u64) -> Result<Home, String> {
    let devices = Devices::from_list(load_devices(storage)?);
    let mut home = Home::from_parts(
        devices,
        load_zones(storage)?,
        load_equipments(storage)?,
        now,
    );
    home.load_recipes(load_recipes(storage)?);
    Ok(home)
}
