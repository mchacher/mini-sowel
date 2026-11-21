//! The devices on disk: `devices.json` in the data folder, a JSON list of Sowel's
//! `DeviceWithDetails`, readable by its owner only. Written under a temporary name, then renamed,
//! so a stop in the middle never leaves half a file. Only the radio loop writes (`Saver`).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use mini_sowel_core::devices::{
    DeviceStore, DeviceWithDetails, Devices, WritePolicy, lock, parse_list,
};

const FILE_NAME: &str = "devices.json";

/// `devices.json` of a data folder.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(dir: &Path) -> Self {
        FileStore {
            path: dir.join(FILE_NAME),
        }
    }

    fn cannot_read(&self, reason: impl std::fmt::Display) -> String {
        format!("devices: {}: cannot read: {reason}", self.path.display())
    }

    fn cannot_write(&self, reason: impl std::fmt::Display) -> String {
        format!("devices: {}: cannot write: {reason}", self.path.display())
    }
}

impl DeviceStore for FileStore {
    /// A missing file is an empty list (first run); a file that cannot be read or parsed is an
    /// error, and the file is left as it is.
    fn load(&self) -> Result<Vec<DeviceWithDetails>, String> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(self.cannot_read(e)),
        };
        parse_list(&text).map_err(|e| self.cannot_read(e))
    }

    fn save(&self, text: &str) -> Result<(), String> {
        let temporary = self.path.with_extension("json.tmp");
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
            fs::rename(&temporary, &self.path)
        })()
        .map_err(|e| self.cannot_write(e))
    }
}

/// The single writer: decides when (`WritePolicy`) and writes, taking the snapshot under the
/// lock and writing it after the lock is released.
pub struct Saver {
    store: FileStore,
    policy: WritePolicy,
}

impl Saver {
    pub fn new(store: FileStore, now: u64) -> Self {
        Saver {
            store,
            policy: WritePolicy::new(now),
        }
    }

    /// Writes the devices if a write is due. A failure is shown and the write is tried again
    /// later, except on stop (`stopping`), where it is the error.
    pub fn tick(
        &mut self,
        devices: &Mutex<Devices>,
        now: u64,
        stopping: bool,
    ) -> Result<(), String> {
        let json = {
            let mut guard = lock(devices);
            self.policy.due(&mut guard, now, stopping)
        };
        let Some(json) = json else {
            return Ok(());
        };
        match self.store.save(&json) {
            Ok(()) => Ok(()),
            Err(message) if stopping => Err(message),
            Err(message) => {
                eprintln!("{message}");
                self.policy.failed(&mut lock(devices), now);
                Ok(())
            }
        }
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
