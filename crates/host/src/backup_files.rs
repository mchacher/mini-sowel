//! The full backup's files on the Mac (spec 035): `BackupFiles` over the data folder, and the
//! clock and pause of the core's `Pacing`. The archive's `zigbee.json` is the host's
//! `network.json` (a network kept aside is `network-<PAN id>.json`); the Mac has no Wi-Fi
//! settings. Staged copies (`<file>.restore`) are readable by their owner only.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use solite_core::backup::{ARCHIVE_FILES, BackupFiles, Pacing, StagedFile, ZIGBEE_FILE};

use crate::serve::monotonic_ms;
use crate::store;

/// The host's name for the archive's `zigbee.json`.
pub const NETWORK_FILE: &str = "network.json";
const MARKER: &str = "restore.pending";
const RESTORE_MARK: &str = "zigbee.restore";
/// The most names tried to keep a network aside (`-2` ... `-100`), then an error.
const SET_ASIDE_TRIES: usize = 100;
const STAGED: &str = "restore";

/// The data folder seen as the archive's files.
#[derive(Clone)]
pub struct DataFiles {
    dir: PathBuf,
}

/// The file of the data folder that holds an archive name.
fn file_name(name: &str) -> &str {
    if name == ZIGBEE_FILE {
        NETWORK_FILE
    } else {
        name
    }
}

fn failed(path: &Path, e: &io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Removes a file; absent is fine.
fn remove_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(failed(path, &e)),
    }
}

fn open_file(path: &Path) -> Result<Option<File>, String> {
    match File::open(path) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(failed(path, &e)),
    }
}

impl DataFiles {
    pub fn new(dir: &Path) -> Self {
        DataFiles {
            dir: dir.to_path_buf(),
        }
    }

    fn live(&self, name: &str) -> PathBuf {
        self.dir.join(file_name(name))
    }

    fn with_suffix(&self, name: &str, suffix: &str) -> PathBuf {
        self.dir.join(format!("{}.{suffix}", file_name(name)))
    }

    fn marker(&self) -> PathBuf {
        self.dir.join(MARKER)
    }
}

/// A staged copy being written: closed and synced by `finish`.
struct Staged {
    file: File,
    path: PathBuf,
}

impl Write for Staged {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.file.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl StagedFile for Staged {
    fn finish(self: Box<Self>) -> Result<(), String> {
        self.file.sync_all().map_err(|e| failed(&self.path, &e))
    }
}

impl BackupFiles for DataFiles {
    fn has_wifi(&self) -> bool {
        false
    }

    fn restarts(&self) -> bool {
        false
    }

    fn open(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        let live = open_file(&self.live(name))?;
        let file = match live {
            Some(file) => Some(file),
            None => open_file(&self.with_suffix(name, "new"))?,
        };
        Ok(file.map(|f| Box::new(f) as Box<dyn Read>))
    }

    fn stage(&mut self, name: &str) -> Result<Box<dyn StagedFile>, String> {
        let path = self.with_suffix(name, STAGED);
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| failed(&path, &e))?;
        // The mode of a file that already existed is not changed by `open`.
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| failed(&path, &e))?;
        Ok(Box::new(Staged { file, path }))
    }

    fn open_staged(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        Ok(open_file(&self.with_suffix(name, STAGED))?.map(|f| Box::new(f) as Box<dyn Read>))
    }

    fn discard(&mut self) -> Result<(), String> {
        for file in ARCHIVE_FILES {
            remove_file(&self.with_suffix(file.name, STAGED))?;
        }
        self.clear_pending()
    }

    fn commit(&mut self, marker: &str) -> Result<(), String> {
        // The store's write path: temporary name, sync, rename.
        store::write(&self.marker(), marker, "restore marker")
    }

    fn pending(&mut self) -> Result<Option<String>, String> {
        match fs::read_to_string(self.marker()) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(failed(&self.marker(), &e)),
        }
    }

    fn promote(&mut self, name: &str) -> Result<(), String> {
        let staged = self.with_suffix(name, STAGED);
        if staged.exists() {
            let live = self.live(name);
            // A rename replaces the file in one step on this system.
            fs::rename(&staged, &live).map_err(|e| failed(&live, &e))?;
        }
        remove_file(&self.with_suffix(name, "new"))?;
        remove_file(&self.with_suffix(name, "tmp"))
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        remove_file(&self.live(name))?;
        remove_file(&self.with_suffix(name, "new"))?;
        remove_file(&self.with_suffix(name, "tmp"))
    }

    fn set_aside(&mut self, name: &str, label: &str) -> Result<(), String> {
        let live = self.live(name);
        if !live.exists() {
            return Ok(());
        }
        // Never over a file kept earlier: `-2`, `-3`... when the name is taken.
        let aside = (1..=SET_ASIDE_TRIES)
            .map(|n| match n {
                1 => self.dir.join(format!("network-{label}.json")),
                n => self.dir.join(format!("network-{label}-{n}.json")),
            })
            .find(|path| !path.exists())
            .ok_or_else(|| format!("{} files already kept for this network", SET_ASIDE_TRIES))?;
        match fs::rename(&live, &aside) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(failed(&live, &e)),
        }
    }

    fn clear_pending(&mut self) -> Result<(), String> {
        remove_file(&self.marker())?;
        // The store's temporary name of the marker, left by a stop in the middle of its write.
        remove_file(&self.marker().with_extension("json.tmp"))
    }

    fn restore_mark(&mut self) -> Result<bool, String> {
        Ok(self.dir.join(RESTORE_MARK).exists())
    }

    fn set_restore_mark(&mut self) -> Result<(), String> {
        let path = self.dir.join(RESTORE_MARK);
        store::write(&path, "restore\n", "restore mark")
    }

    fn clear_restore_mark(&mut self) -> Result<(), String> {
        remove_file(&self.dir.join(RESTORE_MARK))
    }
}

/// The monotonic clock and a 10 ms sleep.
pub struct HostPacing;

impl Pacing for HostPacing {
    fn now_ms(&mut self) -> u64 {
        monotonic_ms()
    }

    fn pause(&mut self) {
        thread::sleep(Duration::from_millis(10));
    }
}

/// Finishes a restore the last run left (R6), before anything reads the data folder. A failure
/// is shown and the start goes on: the marker stays for the next start.
pub fn apply_pending(dir: &Path) {
    let mut files = DataFiles::new(dir);
    if let Err(e) = solite_core::backup::apply_pending(&mut files) {
        log::error!("backup: cannot finish the restore: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("solite-files-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn stage(files: &mut DataFiles, name: &str, text: &str) {
        let mut staged = files.stage(name).unwrap();
        staged.write_all(text.as_bytes()).unwrap();
        staged.finish().unwrap();
    }

    #[test]
    fn the_archives_zigbee_file_is_the_hosts_network_file() {
        let dir = dir("mapping");
        let mut files = DataFiles::new(&dir);
        fs::write(dir.join("network.json"), "live").unwrap();
        let mut text = String::new();
        files
            .open("zigbee.json")
            .unwrap()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "live");
        stage(&mut files, "zigbee.json", "staged");
        assert!(dir.join("network.json.restore").exists());
        files.promote("zigbee.json").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("network.json")).unwrap(),
            "staged"
        );
        assert!(!dir.join("network.json.restore").exists());
        // The network of another PAN id is kept aside under the host's name.
        files.set_aside("zigbee.json", "1A2B").unwrap();
        assert!(dir.join("network-1A2B.json").exists());
        assert!(!dir.join("network.json").exists());
        assert!(!files.has_wifi());
        assert!(!files.restarts());
    }

    #[test]
    fn staged_copies_and_the_marker_are_readable_by_their_owner_only() {
        let dir = dir("mode");
        let mut files = DataFiles::new(&dir);
        // A staged copy that existed with another mode is brought back to 0600.
        fs::write(dir.join("zones.json.restore"), "old").unwrap();
        fs::set_permissions(
            dir.join("zones.json.restore"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        stage(&mut files, "zones.json", "[]");
        stage(&mut files, "zigbee.json", "{}");
        files.commit("zones.json\nend\n").unwrap();
        files.set_restore_mark().unwrap();
        for name in [
            "zones.json.restore",
            "network.json.restore",
            "restore.pending",
            "zigbee.restore",
        ] {
            assert_eq!(mode(&dir.join(name)), 0o600, "{name}");
        }
    }

    #[test]
    fn a_missing_live_file_is_read_from_its_new_copy_and_the_leftovers_go_with_a_promotion() {
        let dir = dir("leftovers");
        let mut files = DataFiles::new(&dir);
        assert!(files.open("zones.json").unwrap().is_none());
        fs::write(dir.join("zones.json.new"), "new").unwrap();
        let mut text = String::new();
        files
            .open("zones.json")
            .unwrap()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "new");
        stage(&mut files, "zones.json", "staged");
        fs::write(dir.join("zones.json.tmp"), "tmp").unwrap();
        files.promote("zones.json").unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("zones.json")).unwrap(),
            "staged"
        );
        assert!(!dir.join("zones.json.new").exists() && !dir.join("zones.json.tmp").exists());
        // A promotion whose copy is gone only cleans; a removal takes the live file too.
        files.promote("zones.json").unwrap();
        files.remove("zones.json").unwrap();
        assert!(!dir.join("zones.json").exists());
    }

    #[test]
    fn a_marker_is_applied_at_start_and_the_restore_mark_survives_a_discard() {
        let dir = dir("apply");
        let mut files = DataFiles::new(&dir);
        fs::write(dir.join("zones.json"), "old").unwrap();
        stage(&mut files, "zones.json", "[1]");
        stage(&mut files, "recipes.json", "[]");
        files.set_restore_mark().unwrap();
        // A copy without a marker is removed, the mark stays.
        apply_pending(&dir);
        assert!(!dir.join("zones.json.restore").exists());
        assert_eq!(fs::read_to_string(dir.join("zones.json")).unwrap(), "old");
        assert!(files.restore_mark().unwrap());
        files.clear_restore_mark().unwrap();
        assert!(!files.restore_mark().unwrap());
    }

    #[test]
    fn a_network_set_aside_never_overwrites_one_kept_earlier() {
        let dir = dir("aside");
        let mut files = DataFiles::new(&dir);
        for (n, expected) in [
            "network-1A2B.json",
            "network-1A2B-2.json",
            "network-1A2B-3.json",
        ]
        .iter()
        .enumerate()
        {
            fs::write(dir.join("network.json"), format!("live {n}")).unwrap();
            files.set_aside("zigbee.json", "1A2B").unwrap();
            assert!(!dir.join("network.json").exists());
            assert_eq!(
                fs::read_to_string(dir.join(expected)).unwrap(),
                format!("live {n}")
            );
        }
        // No live file: nothing to keep.
        files.set_aside("zigbee.json", "1A2B").unwrap();
        assert!(!dir.join("network-1A2B-4.json").exists());
        // Every name taken: an error, the live file stays.
        for n in 2..=SET_ASIDE_TRIES {
            fs::write(dir.join(format!("network-1A2B-{n}.json")), "kept").unwrap();
        }
        fs::write(dir.join("network.json"), "live").unwrap();
        assert!(files.set_aside("zigbee.json", "1A2B").is_err());
        assert_eq!(
            fs::read_to_string(dir.join("network.json")).unwrap(),
            "live"
        );
    }
}
