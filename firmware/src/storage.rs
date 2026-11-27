//! The data partition: FAT with wear levelling, mounted at `/data`, and the engine's `Storage`
//! on it. Every write to it is counted and logged (`flash: <n> writes since boot`, spec 025).

use std::ffi::{CStr, CString};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use esp_idf_svc::sys::{
    WL_INVALID_HANDLE, esp_err_to_name, esp_ptr_external_ram, esp_vfs_fat_info,
    esp_vfs_fat_mount_config_t, esp_vfs_fat_spiflash_mount_rw_wl, wl_handle_t,
};
use solite_core::engine::{
    Storage, WriteCounter, complete_name, flash_line, recover_interrupted_save, temporary_name,
};

/// Where the data partition is mounted.
pub const BASE: &str = "/data";
/// The partition's name in `partitions.csv`.
const LABEL: &str = "storage";

/// The writes to `/data` since boot (spec 025 R2): 4 bytes, stops at its maximum.
static WRITES: WriteCounter = WriteCounter::new();

/// Counts one write (attempted, failed or not) and logs `flash: <n> writes since boot`.
pub(crate) fn counted<T>(result: T) -> T {
    log::info!("{}", flash_line(WRITES.record()));
    result
}

/// Set once the PSRAM-stack refusal has been logged (once per boot).
static PSRAM_STACK_LOGGED: AtomicBool = AtomicBool::new(false);

/// R2 (spec 045): a flash access, a read too, turns the caches and so the PSRAM off, and ESP-IDF
/// stops the chip when the caller's stack is in PSRAM (D59). Every flash call of this file starts
/// with this check: from such a stack it is an error, logged once per boot, not a crash. A guard
/// for a future route; no path in use reaches it.
#[allow(unsafe_code)]
fn flash_allowed() -> Result<(), String> {
    let local = 0u8;
    // SAFETY: only reads the address of a local, to tell which memory it is in.
    let in_psram = unsafe { esp_ptr_external_ram(std::ptr::from_ref(&local).cast()) };
    if !in_psram {
        return Ok(());
    }
    if !PSRAM_STACK_LOGGED.swap(true, Ordering::Relaxed) {
        log::error!("storage: flash access from a PSRAM stack refused");
    }
    Err("flash access from a PSRAM stack refused".to_string())
}

/// The name of an ESP-IDF error code.
#[allow(unsafe_code)]
fn error_name(code: i32) -> String {
    // SAFETY: `esp_err_to_name` returns a pointer to a static, NUL-terminated string (or to
    // "UNKNOWN ERROR"), valid for the whole run.
    unsafe { CStr::from_ptr(esp_err_to_name(code)) }
        .to_string_lossy()
        .into_owned()
}

#[allow(unsafe_code)]
fn try_mount(format_if_mount_failed: bool) -> Result<(), String> {
    let base = CString::new(BASE).map_err(|e| e.to_string())?;
    let label = CString::new(LABEL).map_err(|e| e.to_string())?;
    let config = esp_vfs_fat_mount_config_t {
        format_if_mount_failed,
        max_files: 4,
        // 0: one allocation unit is one sector of the wear-levelling layer.
        allocation_unit_size: 0,
        disk_status_check_enable: false,
        use_one_fat: false,
    };
    let mut handle: wl_handle_t = WL_INVALID_HANDLE;
    // SAFETY: `base` and `label` are valid NUL-terminated strings that outlive the call; `config`
    // and `handle` are valid for the call, which only reads the first and writes the second.
    // The handle is kept mounted for the whole run (never unmounted), so it is not stored.
    let code = unsafe {
        esp_vfs_fat_spiflash_mount_rw_wl(base.as_ptr(), label.as_ptr(), &config, &mut handle)
    };
    if code == 0 {
        Ok(())
    } else {
        Err(error_name(code))
    }
}

/// What mounting gave.
pub struct Mounted {
    /// Why the partition was formatted; `None` when it mounted as it was.
    pub formatted: Option<String>,
}

/// Mounts the data partition. A partition that does not mount (blank or corrupt) is formatted;
/// only a failure of that is the error.
pub fn mount() -> Result<Mounted, String> {
    match try_mount(false) {
        Ok(()) => Ok(Mounted { formatted: None }),
        Err(reason) => {
            try_mount(true).map_err(|e| format!("storage: cannot mount or format {BASE}: {e}"))?;
            Ok(Mounted {
                formatted: Some(format!("mount failed: {reason}")),
            })
        }
    }
}

/// Bytes used and total of the mounted partition.
#[allow(unsafe_code)]
pub fn usage() -> Result<(u64, u64), String> {
    flash_allowed()?;
    let base = CString::new(BASE).map_err(|e| e.to_string())?;
    let (mut total, mut free) = (0u64, 0u64);
    // SAFETY: `base` is a valid NUL-terminated string; `total` and `free` are valid for writes
    // for the duration of the call.
    let code = unsafe { esp_vfs_fat_info(base.as_ptr(), &mut total, &mut free) };
    if code == 0 {
        Ok((total.saturating_sub(free), total))
    } else {
        Err(format!(
            "storage: cannot read the usage: {}",
            error_name(code)
        ))
    }
}

/// The engine's files in `/data`.
pub struct FatStorage {
    dir: PathBuf,
}

impl FatStorage {
    pub fn new() -> Self {
        FatStorage {
            dir: PathBuf::from(BASE),
        }
    }
}

fn what(name: &str) -> &str {
    name.trim_end_matches(".json")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn put(
    path: &Path,
    temporary: &Path,
    next: &mut dyn FnMut() -> io::Result<Option<String>>,
) -> io::Result<()> {
    let mut file = File::create(temporary)?;
    while let Some(chunk) = next()? {
        file.write_all(chunk.as_bytes())?;
    }
    file.sync_all()?;
    drop(file);
    finish_save(path, temporary)
}

/// The same save for a whole file of bytes (an installed script recipe, spec 042).
fn put_bytes(path: &Path, temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let mut file = File::create(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    finish_save(path, temporary)
}

/// The tail of a save: the complete file takes its name (see `put`).
fn finish_save(path: &Path, temporary: &Path) -> io::Result<()> {
    // The content is complete and on the partition: it takes the name `.new`, which only a
    // complete file bears. FAT refuses a rename onto an existing file, so the old file goes
    // first. A power cut between the delete and the last rename leaves only `.new`: `read`
    // recovers it. A `.tmp` left by a cut write is never recovered.
    let complete = path.with_file_name(complete_name(&file_name(path)));
    let _ = fs::remove_file(&complete);
    fs::rename(temporary, &complete)?;
    let _ = fs::remove_file(path);
    fs::rename(&complete, path)?;
    Ok(())
}

impl Storage for FatStorage {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        flash_allowed()?;
        let path = self.dir.join(name);
        // A power cut between the delete and the rename of a save leaves only `<name>.new`.
        let complete = self.dir.join(complete_name(name));
        let temporary = self.dir.join(temporary_name(name));
        recover_interrupted_save(
            path.as_path(),
            complete.as_path(),
            temporary.as_path(),
            Path::exists,
            |from, to| counted(fs::rename(from, to)),
            |file| counted(fs::remove_file(file)),
        )
        .map_err(|e| format!("{}: {}: cannot recover: {e}", what(name), path.display()))?;
        match fs::read_to_string(&path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!(
                "{}: {}: cannot read: {e}",
                what(name),
                path.display()
            )),
        }
    }

    fn write(
        &self,
        name: &str,
        next: &mut dyn FnMut() -> io::Result<Option<String>>,
    ) -> Result<(), String> {
        flash_allowed()?;
        let path = self.dir.join(name);
        let temporary = self.dir.join(temporary_name(name));
        counted(
            put(&path, &temporary, next)
                .map_err(|e| format!("{}: {}: cannot write: {e}", what(name), path.display())),
        )
    }

    fn locate(&self, name: &str) -> String {
        self.dir.join(name).display().to_string()
    }

    fn read_bytes(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        flash_allowed()?;
        let path = self.dir.join(name);
        recover(&path)?;
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: cannot read: {e}", path.display())),
        }
    }

    fn write_bytes(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        flash_allowed()?;
        let path = self.dir.join(name);
        let temporary = with_suffix(&path, ".tmp");
        counted(
            put_bytes(&path, &temporary, bytes)
                .map_err(|e| format!("{}: cannot write: {e}", path.display())),
        )
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        flash_allowed()?;
        let path = self.dir.join(name);
        match fs::remove_file(&path) {
            Ok(()) => counted(Ok(())),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => counted(Err(format!("{}: cannot remove: {e}", path.display()))),
        }
    }

    fn list(&self, folder: &str) -> Result<Vec<String>, String> {
        flash_allowed()?;
        let path = self.dir.join(folder);
        let names = |path: &Path| -> Result<Vec<String>, String> {
            match fs::read_dir(path) {
                Ok(entries) => Ok(entries
                    .flatten()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(e) => Err(format!("{}: cannot list: {e}", path.display())),
            }
        };
        // A lone `<name>.new` is a save a power cut interrupted: promoted first, never a `.tmp`.
        for name in names(&path)? {
            if let Some(base) = name.strip_suffix(".new") {
                recover(&path.join(base))?;
            }
        }
        let mut listed = names(&path)?;
        listed.retain(|name| !name.ends_with(".tmp") && !name.ends_with(".new"));
        listed.sort();
        Ok(listed)
    }

    fn usage(&self) -> Result<(u64, Option<u64>), String> {
        flash_allowed()?;
        usage().map(|(used, total)| (used, Some(total)))
    }
}

/// `path` and `suffix`: `a.srs` and `.tmp` make `a.srs.tmp`.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Finishes a save a power cut interrupted (see `recover_interrupted_save`).
fn recover(path: &Path) -> Result<(), String> {
    recover_interrupted_save(
        path,
        &with_suffix(path, ".new"),
        &with_suffix(path, ".tmp"),
        Path::exists,
        |from, to| counted(fs::rename(from, to)),
        |file| counted(fs::remove_file(file)),
    )
    .map(|_| ())
    .map_err(|e| format!("{}: cannot recover: {e}", path.display()))
}

/// Writes a whole file (the seed of the QEMU image, the radio image's Zigbee backup) in chunks of
/// 4 KB, so that no copy of it is made; errors name the file.
#[cfg(any(feature = "sim", feature = "radio"))]
pub fn write_text(name: &str, text: &str) -> Result<(), String> {
    const CHUNK: usize = 4096;
    let mut position = 0;
    FatStorage::new().write(name, &mut || {
        if position >= text.len() {
            return Ok(None);
        }
        let mut end = (position + CHUNK).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let chunk = text[position..end].to_string();
        position = end;
        Ok(Some(chunk))
    })
}

/// Renames a file of the data partition; FAT refuses a rename onto an existing file, so the
/// target goes first.
#[cfg(feature = "radio")]
pub fn rename(from: &str, to: &str) -> Result<(), String> {
    flash_allowed()?;
    let (from_path, to_path) = (Path::new(BASE).join(from), Path::new(BASE).join(to));
    if fs::remove_file(&to_path).is_ok() {
        counted(());
    }
    counted(
        fs::rename(&from_path, &to_path)
            .map_err(|e| format!("{}: cannot rename to {}: {e}", from_path.display(), to)),
    )
}

/// Removes a file of the data partition (the console's `wifi-forget`, the boot marker); a file
/// already absent is removed too, and that is not counted as a write.
#[cfg(feature = "radio")]
pub fn remove(name: &str) -> Result<(), String> {
    flash_allowed()?;
    let path = Path::new(BASE).join(name);
    match fs::remove_file(&path) {
        Ok(()) => counted(Ok(())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => counted(Err(format!("{}: cannot remove: {e}", path.display()))),
    }
}

/// Writes an empty file in the data partition (the boot marker), counted.
#[cfg(feature = "radio")]
pub fn touch(name: &str) -> Result<(), String> {
    flash_allowed()?;
    let path = Path::new(BASE).join(name);
    counted(fs::write(&path, b"").map_err(|e| format!("{}: cannot write: {e}", path.display())))
}
