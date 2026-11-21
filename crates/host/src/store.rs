//! The home on disk, in the data folder: `devices.json` (a JSON list of Sowel's
//! `DeviceWithDetails`), `zones.json` (`Zone`s) and `equipments.json` (equipments with their
//! bindings), each readable by its owner only. Written under a temporary name, then renamed, so
//! a stop in the middle never leaves half a file. Only the radio loop writes (`Saver`).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use mini_sowel_core::devices::{
    DeviceStore, DeviceWithDetails, Devices, WritePolicy, lock, parse_list,
};
use mini_sowel_core::equipments::{self, StoredEquipment};
use mini_sowel_core::home::{Home, ListCursor};
use mini_sowel_core::zones::{self, Zone};

const FILE_NAME: &str = "devices.json";
const ZONES_FILE_NAME: &str = "zones.json";
const EQUIPMENTS_FILE_NAME: &str = "equipments.json";
/// A zones or equipments file that could not be written is tried again after this, in
/// milliseconds (never in a tight loop).
const RETRY_MS: u64 = 60 * 1000;

/// The files of a data folder.
pub struct FileStore {
    path: PathBuf,
    zones: PathBuf,
    equipments: PathBuf,
}

/// Reads a file; a missing file is `None` (first run), one that cannot be read is the error.
fn read(path: &Path, what: &str) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{what}: {}: cannot read: {e}", path.display())),
    }
}

/// Writes a file under a temporary name, then renames it.
fn write(path: &Path, text: &str, what: &str) -> Result<(), String> {
    let temporary = path.with_extension("json.tmp");
    (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        // The mode of a file that already existed is not changed by `open`.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })()
    .map_err(|e| format!("{what}: {}: cannot write: {e}", path.display()))
}

/// Writes a file from a cursor, one chunk at a time under a temporary name, then renames it.
/// `next` takes what the cursor gives (and so the home, for as long as one chunk is built, never
/// while writing).
fn write_chunks(
    path: &Path,
    mut next: impl FnMut() -> std::io::Result<Option<String>>,
    what: &str,
) -> Result<(), String> {
    let temporary = path.with_extension("json.tmp");
    (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        while let Some(chunk) = next()? {
            file.write_all(chunk.as_bytes())?;
        }
        file.sync_all()?;
        fs::rename(&temporary, path)
    })()
    .map_err(|e| format!("{what}: {}: cannot write: {e}", path.display()))
}

impl FileStore {
    pub fn new(dir: &Path) -> Self {
        FileStore {
            path: dir.join(FILE_NAME),
            zones: dir.join(ZONES_FILE_NAME),
            equipments: dir.join(EQUIPMENTS_FILE_NAME),
        }
    }

    fn cannot_read(&self, reason: impl std::fmt::Display) -> String {
        format!("devices: {}: cannot read: {reason}", self.path.display())
    }

    /// The zones; the root alone when there is no file. A file that cannot be read or parsed is
    /// an error, and the file is left as it is.
    pub fn load_zones(&self) -> Result<Vec<Zone>, String> {
        let Some(text) = read(&self.zones, "zones")? else {
            return Ok(Vec::new());
        };
        zones::parse_list(&text)
            .map_err(|e| format!("zones: {}: cannot read: {e}", self.zones.display()))
    }

    #[cfg(test)]
    pub fn save_zones(&self, text: &str) -> Result<(), String> {
        write(&self.zones, text, "zones")
    }

    pub fn load_equipments(&self) -> Result<Vec<StoredEquipment>, String> {
        let Some(text) = read(&self.equipments, "equipments")? else {
            return Ok(Vec::new());
        };
        equipments::parse_list(&text).map_err(|e| {
            format!(
                "equipments: {}: cannot read: {e}",
                self.equipments.display()
            )
        })
    }

    #[cfg(test)]
    pub fn save_equipments(&self, text: &str) -> Result<(), String> {
        write(&self.equipments, text, "equipments")
    }

    /// The whole home, made consistent; a file that cannot be read stops everything.
    pub fn load_home(&self, now: u64) -> Result<Home, String> {
        let devices = Devices::from_list(self.load()?);
        Ok(Home::from_parts(
            devices,
            self.load_zones()?,
            self.load_equipments()?,
            now,
        ))
    }
}

impl DeviceStore for FileStore {
    /// A missing file is an empty list (first run); a file that cannot be read or parsed is an
    /// error, and the file is left as it is.
    fn load(&self) -> Result<Vec<DeviceWithDetails>, String> {
        let Some(text) = read(&self.path, "devices")? else {
            return Ok(Vec::new());
        };
        parse_list(&text).map_err(|e| self.cannot_read(e))
    }

    fn save(&self, text: &str) -> Result<(), String> {
        write(&self.path, text, "devices")
    }
}

/// The single writer: decides when (`WritePolicy` for the devices; at once for zones and
/// equipments) and writes, taking the snapshots under the lock and writing them after the lock
/// is released.
pub struct Saver {
    store: FileStore,
    policy: WritePolicy,
    /// `pair` writes the devices only: zones and equipments are written by `--serve`.
    devices_only: bool,
    /// Not before this time are the zones and equipments tried again after a failure.
    retry_at: u64,
}

impl Saver {
    pub fn new(store: FileStore, now: u64) -> Self {
        Saver {
            store,
            policy: WritePolicy::new(now),
            devices_only: false,
            retry_at: 0,
        }
    }

    /// A saver that writes `devices.json` only.
    pub fn devices_only(store: FileStore, now: u64) -> Self {
        Saver {
            devices_only: true,
            ..Saver::new(store, now)
        }
    }

    /// Writes what is due. A failure is shown and the write is tried again later, except on stop
    /// (`stopping`), where it is the error. Each file is written chunk by chunk from a cursor: the
    /// home is taken for one chunk, then let go, then the chunk is written (memory ceiling, spec
    /// 010 R7.15).
    pub fn tick(&mut self, home: &Mutex<Home>, now: u64, stopping: bool) -> Result<(), String> {
        let devices = self.policy.due_stream(&mut lock(home), now, stopping);
        if let Some(cursor) = devices
            && let Err(message) = self.write(home, cursor, &self.store.path)
        {
            if stopping {
                return Err(message);
            }
            eprintln!("{message}");
            self.policy.failed(&mut lock(home).devices, now);
        }
        if self.devices_only || !(stopping || now >= self.retry_at) {
            return Ok(());
        }
        let zones = lock(home).take_zones_cursor();
        if let Some(cursor) = zones
            && let Err(message) = self.write(home, cursor, &self.store.zones)
        {
            if stopping {
                return Err(message);
            }
            eprintln!("{message}");
            lock(home).mark_zones_changed();
            self.retry_at = now + RETRY_MS;
        }
        let equipments = lock(home).take_equipments_cursor();
        if let Some(cursor) = equipments
            && let Err(message) = self.write(home, cursor, &self.store.equipments)
        {
            if stopping {
                return Err(message);
            }
            eprintln!("{message}");
            lock(home).mark_equipments_changed();
            self.retry_at = now + RETRY_MS;
        }
        Ok(())
    }

    fn write(&self, home: &Mutex<Home>, mut cursor: ListCursor, path: &Path) -> Result<(), String> {
        let what = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("home")
            .to_string();
        write_chunks(path, || cursor.next_chunk(&lock(home)), &what)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mini-sowel-store-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn json() -> String {
        Devices::from_list(devices()).to_json().unwrap()
    }

    fn devices() -> Vec<DeviceWithDetails> {
        let mut devices = Devices::new();
        let mut n = 0u8;
        let mut random = move |buffer: &mut [u8]| {
            n += 1;
            buffer.fill(n);
            Ok(())
        };
        let eui = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78];
        devices
            .identified(
                eui,
                Some("eWeLink"),
                Some("TH01"),
                1_795_252_323_120,
                &mut random,
            )
            .unwrap();
        devices
            .reporting_configured(eui, &[0x0402, 0x0006], 1_795_252_323_120, &mut random)
            .unwrap();
        devices.snapshot()
    }

    fn home_with_garage() -> Home {
        let mut random = {
            let mut n = 0u8;
            move |buffer: &mut [u8]| {
                n += 1;
                buffer.fill(n);
                Ok(())
            }
        };
        let mut home = Home::from_devices(Devices::from_list(devices()), 1_795_252_323_120);
        let zone = home
            .create_zone(
                mini_sowel_core::zones::NewZone {
                    name: "Garage".into(),
                    ..Default::default()
                },
                1_795_252_323_120,
                &mut random,
            )
            .unwrap();
        home.create_equipment(
            mini_sowel_core::home::NewEquipment {
                name: "Climate".into(),
                kind: "sensor".into(),
                zone_id: zone.id.to_string(),
                device_ids: vec![home.devices.list()[0].device.id.to_string()],
                ..Default::default()
            },
            1_795_252_323_120,
            &mut random,
        )
        .unwrap();
        home
    }

    #[test]
    fn zones_and_equipments_are_saved_and_loaded_the_same() {
        let dir = dir("home-roundtrip");
        let store = FileStore::new(&dir);
        let mut home = home_with_garage();
        let home_before = (
            home.zones().list().to_vec(),
            home.equipments().list().to_vec(),
        );
        // No file yet: the root zone is made, nothing else.
        let first = store.load_home(1).unwrap();
        assert_eq!(first.zones().list().len(), 1);
        assert!(first.equipments().list().is_empty());
        store.save(&home.devices.to_json().unwrap()).unwrap();
        store.save_zones(&home.take_zones_json().unwrap()).unwrap();
        store
            .save_equipments(&home.take_equipments_json().unwrap())
            .unwrap();
        let loaded = store.load_home(2).unwrap();
        assert_eq!(loaded.zones().list(), home_before.0.as_slice());
        assert_eq!(loaded.equipments().list(), home_before.1.as_slice());
        assert_eq!(loaded.devices.list(), home.devices.list());
        assert_eq!(loaded.changes(), (false, false), "nothing to repair");
    }

    #[test]
    fn a_zones_or_equipments_file_that_does_not_parse_is_the_error_and_stays_as_it_is() {
        for (name, what) in [("zones.json", "zones"), ("equipments.json", "equipments")] {
            let dir = dir(&format!("garbage-{what}"));
            let file = dir.join(name);
            fs::write(&file, "[ not json").unwrap();
            let message = FileStore::new(&dir).load_home(1).expect_err("an error");
            assert!(
                message.starts_with(&format!("{what}: {}: cannot read: ", file.display())),
                "{message}"
            );
            assert_eq!(fs::read_to_string(&file).unwrap(), "[ not json");
        }
        // A file of the wrong shape is refused too.
        let dir = dir("wrong-shape");
        fs::write(dir.join("zones.json"), r#"[{"id":"x"}]"#).unwrap();
        assert!(FileStore::new(&dir).load_home(1).is_err());
    }

    #[test]
    fn zones_and_equipments_are_readable_by_their_owner_only() {
        let dir = dir("home-mode");
        let store = FileStore::new(&dir);
        let mut home = home_with_garage();
        store.save_zones(&home.take_zones_json().unwrap()).unwrap();
        store
            .save_equipments(&home.take_equipments_json().unwrap())
            .unwrap();
        for name in ["zones.json", "equipments.json"] {
            let file = dir.join(name);
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        store.save_zones(&home.zones().to_json().unwrap()).unwrap();
        store
            .save_equipments(&home.equipments().to_json().unwrap())
            .unwrap();
        for name in ["zones.json", "equipments.json"] {
            let mode = fs::metadata(dir.join(name)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{name}");
            assert!(!dir.join(name.replace(".json", ".json.tmp")).exists());
        }
    }

    #[test]
    fn the_saver_writes_zones_and_equipments_at_once_and_devices_only_for_pair() {
        let dir = dir("saver");
        let home = std::sync::Mutex::new(home_with_garage());
        let mut saver = Saver::new(FileStore::new(&dir), 0);
        lock(&home).devices.mark_values_changed();
        saver.tick(&home, 1, true).unwrap();
        for name in ["devices.json", "zones.json", "equipments.json"] {
            assert!(dir.join(name).exists(), "{name}");
        }
        // Nothing changed: the files are not rewritten.
        fs::remove_file(dir.join("zones.json")).unwrap();
        saver.tick(&home, 2, false).unwrap();
        assert!(!dir.join("zones.json").exists());
        // A change is written at the next tick, not 10 minutes later.
        lock(&home).mark_zones_changed();
        saver.tick(&home, 3, false).unwrap();
        assert!(dir.join("zones.json").exists());

        // `--pair` writes the devices only.
        let dir = self::dir("saver-pair");
        let home = std::sync::Mutex::new(home_with_garage());
        let mut saver = Saver::devices_only(FileStore::new(&dir), 0);
        lock(&home).devices.mark_values_changed();
        saver.tick(&home, 1, true).unwrap();
        assert!(dir.join("devices.json").exists());
        assert!(!dir.join("zones.json").exists() && !dir.join("equipments.json").exists());
        assert_eq!(lock(&home).changes(), (true, true), "left for --serve");
    }

    #[test]
    fn a_failed_zones_write_is_tried_again_a_minute_later_not_in_a_loop() {
        let dir = dir("saver-retry");
        let home = std::sync::Mutex::new(home_with_garage());
        let mut saver = Saver::new(FileStore::new(&dir), 0);
        // A directory where the file should be: the rename fails.
        fs::create_dir_all(dir.join("zones.json")).unwrap();
        saver.tick(&home, 1000, false).unwrap();
        assert!(lock(&home).changes().0, "due again");
        fs::remove_dir_all(dir.join("zones.json")).unwrap();
        saver.tick(&home, 2000, false).unwrap();
        assert!(!dir.join("zones.json").exists(), "not before a minute");
        saver.tick(&home, 1000 + RETRY_MS, false).unwrap();
        assert!(dir.join("zones.json").exists());
        // On stop, a failure is the error.
        fs::remove_file(dir.join("zones.json")).unwrap();
        fs::create_dir_all(dir.join("zones.json")).unwrap();
        lock(&home).mark_zones_changed();
        assert!(saver.tick(&home, 5, true).is_err());
    }

    #[test]
    fn a_chunk_that_cannot_be_built_leaves_the_file_as_it_was_and_is_never_renamed() {
        let dir = dir("chunk-error");
        let file = dir.join("zones.json");
        // No file yet: a failure after a first chunk leaves none.
        let mut chunks = vec![
            Ok(Some("[{".to_string())),
            Err(std::io::Error::other("boom")),
        ]
        .into_iter();
        let result = write_chunks(&file, || chunks.next().unwrap(), "zones");
        assert!(result.unwrap_err().starts_with("zones: "));
        assert!(!file.exists(), "a truncated file is never put in place");
        // A file that existed is kept as it was.
        fs::write(&file, "[1]").unwrap();
        let mut chunks = vec![
            Ok(Some("[2,".to_string())),
            Err(std::io::Error::other("boom")),
        ]
        .into_iter();
        assert!(write_chunks(&file, || chunks.next().unwrap(), "zones").is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "[1]");
        // A complete list replaces it.
        let mut chunks = vec![
            Ok(Some("[2,".to_string())),
            Ok(Some("3]".to_string())),
            Ok(None),
        ]
        .into_iter();
        write_chunks(&file, || chunks.next().unwrap(), "zones").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "[2,3]");
    }

    #[test]
    fn save_then_load_gives_the_same_devices() {
        let dir = dir("roundtrip");
        let store = FileStore::new(&dir);
        assert_eq!(store.load().unwrap(), Vec::new(), "no file: no device");
        store.save(&json()).unwrap();
        assert_eq!(store.load().unwrap(), devices());
    }

    #[test]
    fn a_file_that_does_not_parse_is_the_error_and_stays_as_it_is() {
        let dir = dir("garbage");
        let file = dir.join("devices.json");
        fs::write(&file, "{ not json").unwrap();
        let message = FileStore::new(&dir).load().unwrap_err();
        assert!(
            message.starts_with(&format!("devices: {}: cannot read: ", file.display())),
            "{message}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
    }

    #[test]
    fn the_file_is_readable_by_its_owner_only() {
        let dir = dir("mode");
        let store = FileStore::new(&dir);
        store.save(&json()).unwrap();
        // Also when the file existed with another mode.
        let file = dir.join("devices.json");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        store.save(&json()).unwrap();
        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
