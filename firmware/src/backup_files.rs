//! The full backup's files on the box (spec 035): `BackupFiles` over `/data` with `std::fs`, and
//! the core's `Pacing` (a monotonic clock and a 10 ms sleep). The archive's names are the files'
//! names. FAT refuses a rename onto an existing file, so the destination goes first; every write
//! is counted in the flash write counter like the engine's.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(feature = "radio")]
use std::sync::OnceLock;
#[cfg(feature = "radio")]
use std::time::{Duration, Instant};

#[cfg(feature = "radio")]
use solite_core::backup::Pacing;
use solite_core::backup::{ARCHIVE_FILES, BackupFiles, StagedFile};
use solite_core::engine::{Storage, complete_name, temporary_name};

use crate::storage::{BASE, FatStorage, counted};

const MARKER: &str = "restore.pending";
const RESTORE_MARK: &str = "zigbee.restore";
/// The most names tried to keep a network aside (`-2` ... `-100`), then an error.
const SET_ASIDE_TRIES: usize = 100;
const STAGED: &str = "restore";

/// The data partition seen as the archive's files.
pub struct DataFiles {
    dir: PathBuf,
}

impl DataFiles {
    pub fn new() -> Self {
        DataFiles {
            dir: PathBuf::from(BASE),
        }
    }

    fn live(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn staged(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.{STAGED}"))
    }
}

fn failed(path: &Path, e: &io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Removes a file, counted when there was one; absent is fine.
fn remove_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => counted(Ok(())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => counted(Err(failed(path, &e))),
    }
}

fn open_file(path: &Path) -> Result<Option<File>, String> {
    match File::open(path) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(failed(path, &e)),
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
        cfg!(feature = "radio")
    }

    fn restarts(&self) -> bool {
        true
    }

    fn open(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        // A save cut between the removal and the rename leaves only `<name>.new`.
        let file = match open_file(&self.live(name))? {
            Some(file) => Some(file),
            None => open_file(&self.dir.join(complete_name(name)))?,
        };
        Ok(file.map(|f| Box::new(f) as Box<dyn Read>))
    }

    fn stage(&mut self, name: &str) -> Result<Box<dyn StagedFile>, String> {
        let path = self.staged(name);
        let file = counted(File::create(&path)).map_err(|e| failed(&path, &e))?;
        Ok(Box::new(Staged { file, path }))
    }

    fn open_staged(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String> {
        Ok(open_file(&self.staged(name))?.map(|f| Box::new(f) as Box<dyn Read>))
    }

    fn discard(&mut self) -> Result<(), String> {
        for file in ARCHIVE_FILES {
            remove_file(&self.staged(file.name))?;
        }
        self.clear_pending()
    }

    fn commit(&mut self, marker: &str) -> Result<(), String> {
        // The engine's own write path: temporary file, `.new`, rename.
        let mut once = Some(marker.to_string());
        FatStorage::new().write(MARKER, &mut || Ok(once.take()))
    }

    fn pending(&mut self) -> Result<Option<String>, String> {
        // `read` also recovers a marker cut between its removal and its rename.
        FatStorage::new().read(MARKER)
    }

    fn promote(&mut self, name: &str) -> Result<(), String> {
        let staged = self.staged(name);
        if staged.exists() {
            let live = self.live(name);
            remove_file(&live)?;
            counted(fs::rename(&staged, &live)).map_err(|e| failed(&live, &e))?;
        }
        remove_file(&self.dir.join(complete_name(name)))?;
        remove_file(&self.dir.join(temporary_name(name)))
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        remove_file(&self.live(name))?;
        remove_file(&self.dir.join(complete_name(name)))?;
        remove_file(&self.dir.join(temporary_name(name)))
    }

    fn set_aside(&mut self, name: &str, label: &str) -> Result<(), String> {
        let live = self.live(name);
        if !live.exists() {
            return Ok(());
        }
        // Never over a file kept earlier: `-2`, `-3`... when the name is taken.
        let aside = (1..=SET_ASIDE_TRIES)
            .map(|n| match n {
                1 => self.dir.join(format!("zigbee-{label}.json")),
                n => self.dir.join(format!("zigbee-{label}-{n}.json")),
            })
            .find(|path| !path.exists())
            .ok_or_else(|| format!("{SET_ASIDE_TRIES} files already kept for this network"))?;
        counted(fs::rename(&live, &aside)).map_err(|e| failed(&live, &e))
    }

    fn clear_pending(&mut self) -> Result<(), String> {
        remove_file(&self.dir.join(MARKER))?;
        remove_file(&self.dir.join(complete_name(MARKER)))?;
        remove_file(&self.dir.join(temporary_name(MARKER)))
    }

    fn restore_mark(&mut self) -> Result<bool, String> {
        Ok(self.dir.join(RESTORE_MARK).exists())
    }

    fn set_restore_mark(&mut self) -> Result<(), String> {
        let path = self.dir.join(RESTORE_MARK);
        counted(fs::write(&path, b"")).map_err(|e| failed(&path, &e))
    }

    fn clear_restore_mark(&mut self) -> Result<(), String> {
        remove_file(&self.dir.join(RESTORE_MARK))
    }
}

/// The monotonic clock and a 10 ms sleep (which lets the idle task feed the watchdog).
#[cfg(feature = "radio")]
pub struct BoxPacing;

#[cfg(feature = "radio")]
impl Pacing for BoxPacing {
    fn now_ms(&mut self) -> u64 {
        static START: OnceLock<Instant> = OnceLock::new();
        u64::try_from(START.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn pause(&mut self) {
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Finishes a restore the last run left (R6), right after the partition is mounted and before
/// anything reads it. A failure is logged and the start goes on: the marker stays.
pub fn apply_pending() {
    if let Err(e) = solite_core::backup::apply_pending(&mut DataFiles::new()) {
        log::error!("backup: cannot finish the restore: {e}");
    }
}
