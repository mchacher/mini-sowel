//! The home on disk, in the data folder: `devices.json` (a JSON list of Sowel's
//! `DeviceWithDetails`), `zones.json` (`Zone`s), `equipments.json` (equipments with their
//! bindings) and `recipes.json` (the recipe instances), each readable by its owner only. Written under a temporary name, then renamed, so
//! a stop in the middle never leaves half a file. Only the radio loop writes (`Saver`, in the
//! core's `engine`); this file is the Mac's `Storage`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use solite_core::devices::{DeviceStore, DeviceWithDetails};
use solite_core::engine::{self, Storage};
use solite_core::home::Home;
#[cfg(test)]
use solite_core::recipes::RecipeInstance;

const FILE_NAME: &str = engine::DEVICES_FILE;
#[cfg(test)]
const ZONES_FILE_NAME: &str = engine::ZONES_FILE;
#[cfg(test)]
const EQUIPMENTS_FILE_NAME: &str = engine::EQUIPMENTS_FILE;
#[cfg(test)]
const RECIPES_FILE_NAME: &str = engine::RECIPES_FILE;

/// Where a write failure of the saver is shown: the standard error, as always.
pub fn show_on_stderr(message: &str) {
    eprintln!("{message}");
}

/// The files of a data folder.
pub struct FileStore {
    dir: PathBuf,
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
            dir: dir.to_path_buf(),
        }
    }

    #[cfg(test)]
    pub fn save_zones(&self, text: &str) -> Result<(), String> {
        write(&self.dir.join(ZONES_FILE_NAME), text, "zones")
    }

    #[cfg(test)]
    pub fn save_equipments(&self, text: &str) -> Result<(), String> {
        write(&self.dir.join(EQUIPMENTS_FILE_NAME), text, "equipments")
    }

    /// The recipe instances; none when there is no file. A file that cannot be read or parsed is
    /// an error, and the file is left as it is.
    #[cfg(test)]
    pub fn load_recipes(&self) -> Result<Vec<RecipeInstance>, String> {
        engine::load_recipes(self)
    }

    #[cfg(test)]
    pub fn save_recipes(&self, text: &str) -> Result<(), String> {
        write(&self.dir.join(RECIPES_FILE_NAME), text, "recipes")
    }

    /// The whole home, made consistent; a file that cannot be read stops everything. The recipe
    /// instances are loaded, not started: the caller starts them (`Home::start_recipes`).
    pub fn load_home(&self, now: u64) -> Result<Home, String> {
        engine::load_home(self, now)
    }
}

impl Storage for FileStore {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        read(&self.dir.join(name), name.trim_end_matches(".json"))
    }

    fn write(
        &self,
        name: &str,
        next: &mut dyn FnMut() -> std::io::Result<Option<String>>,
    ) -> Result<(), String> {
        write_chunks(&self.dir.join(name), next, name.trim_end_matches(".json"))
    }

    fn locate(&self, name: &str) -> String {
        self.dir.join(name).display().to_string()
    }
}

impl DeviceStore for FileStore {
    /// A missing file is an empty list (first run); a file that cannot be read or parsed is an
    /// error, and the file is left as it is.
    fn load(&self) -> Result<Vec<DeviceWithDetails>, String> {
        engine::load_devices(self)
    }

    fn save(&self, text: &str) -> Result<(), String> {
        write(&self.dir.join(FILE_NAME), text, "devices")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solite_core::devices::{Devices, lock};
    use solite_core::engine::{RETRY_MS, Saver};

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("solite-store-{}-{name}", std::process::id()));
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
                solite_core::zones::NewZone {
                    name: "Garage".into(),
                    ..Default::default()
                },
                1_795_252_323_120,
                &mut random,
            )
            .unwrap();
        home.create_equipment(
            solite_core::home::NewEquipment {
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

    fn instance(n: u8) -> RecipeInstance {
        RecipeInstance {
            id: solite_core::devices::Id::from_random([n; 16]),
            recipe_id: "motion-light".into(),
            params: serde_json::json!({"zone": "z", "timeout": "20s"})
                .to_string()
                .into(),
            enabled: n % 2 == 1,
            created_at: 1_795_252_323_120,
        }
    }

    #[test]
    fn recipes_are_saved_and_loaded_the_same_with_the_owner_only_mode() {
        let dir = dir("recipes-roundtrip");
        let store = FileStore::new(&dir);
        // No file: no instance.
        assert!(store.load_recipes().unwrap().is_empty());
        let list = vec![instance(1), instance(2)];
        store
            .save_recipes(&serde_json::to_string(&list).unwrap())
            .unwrap();
        assert_eq!(store.load_recipes().unwrap(), list);
        let mode = fs::metadata(dir.join("recipes.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        assert!(!dir.join("recipes.json.tmp").exists());
        // The instances reach the home, not started.
        let home = store.load_home(1).unwrap();
        assert_eq!(home.recipes().len(), 2);
    }

    #[test]
    fn a_recipes_file_that_does_not_parse_is_the_error_and_stays_as_it_is() {
        let dir = dir("recipes-garbage");
        let file = dir.join("recipes.json");
        fs::write(&file, "[ not json").unwrap();
        let message = FileStore::new(&dir).load_home(1).expect_err("an error");
        assert!(
            message.starts_with(&format!("recipes: {}: cannot read: ", file.display())),
            "{message}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "[ not json");
    }

    #[test]
    fn the_saver_writes_recipes_at_once_when_they_changed() {
        let dir = dir("saver-recipes");
        let home = std::sync::Mutex::new(home_with_garage());
        let mut saver = Saver::new(FileStore::new(&dir), 0);
        saver.tick(&home, 1, true).unwrap();
        assert!(!dir.join("recipes.json").exists(), "nothing to write yet");
        lock(&home).mark_recipes_changed();
        saver.tick(&home, 2, false).unwrap();
        assert!(dir.join("recipes.json").exists());
        assert_eq!(fs::read_to_string(dir.join("recipes.json")).unwrap(), "[]");
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
