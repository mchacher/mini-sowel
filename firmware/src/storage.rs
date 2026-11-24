//! The data partition: FAT with wear levelling, mounted at `/data`, and the engine's `Storage`
//! on it. Every write to it is counted and logged (`flash: <n> writes since boot`, spec 025).

use std::ffi::{CStr, CString};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use esp_idf_svc::sys::{
    WL_INVALID_HANDLE, esp_err_to_name, esp_vfs_fat_info, esp_vfs_fat_mount_config_t,
    esp_vfs_fat_spiflash_mount_rw_wl, wl_handle_t,
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
fn counted<T>(result: T) -> T {
    log::info!("{}", flash_line(WRITES.record()));
    result
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
    let path = Path::new(BASE).join(name);
    counted(fs::write(&path, b"").map_err(|e| format!("{}: cannot write: {e}", path.display())))
}
