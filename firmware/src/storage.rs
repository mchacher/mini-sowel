//! The data partition: FAT with wear levelling, mounted at `/data`, and the engine's `Storage`
//! on it.

use std::ffi::{CStr, CString};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use esp_idf_svc::sys::{
    WL_INVALID_HANDLE, esp_err_to_name, esp_vfs_fat_info, esp_vfs_fat_mount_config_t,
    esp_vfs_fat_spiflash_mount_rw_wl, wl_handle_t,
};
use solite_core::engine::{Storage, recover_interrupted_save, temporary_name};

/// Where the data partition is mounted.
pub const BASE: &str = "/data";
/// The partition's name in `partitions.csv`.
const LABEL: &str = "storage";

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
    // FAT keeps no permissions; a rename onto an existing file may be refused: then the old
    // file goes first (the new one is complete and on the partition by now). A power cut between
    // the two leaves only the temporary file: `read` recovers it.
    if fs::rename(temporary, path).is_err() {
        let _ = fs::remove_file(path);
        fs::rename(temporary, path)?;
    }
    Ok(())
}

impl Storage for FatStorage {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        let path = self.dir.join(name);
        // A power cut between the delete and the rename of a save leaves only `<name>.tmp`.
        let temporary = self.dir.join(temporary_name(name));
        recover_interrupted_save(
            path.as_path(),
            temporary.as_path(),
            Path::exists,
            |from, to| fs::rename(from, to),
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
        put(&path, &temporary, next)
            .map_err(|e| format!("{}: {}: cannot write: {e}", what(name), path.display()))
    }

    fn locate(&self, name: &str) -> String {
        self.dir.join(name).display().to_string()
    }
}

/// Writes a whole file (the seed of the QEMU image) in chunks of 4 KB, so that no copy of it is
/// made; errors name the file.
#[cfg(feature = "sim")]
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
