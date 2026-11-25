//! The full backup of the box (spec 035, D56): one archive file that holds the home, the Zigbee
//! network's keys and the Wi-Fi settings, streamed both ways in chunks of 4 KB, the secrets
//! encrypted and the whole file authenticated by a password.
//!
//! - `format`: the archive's bytes (R3): header, entries, manifest, MAC;
//! - `crypto`: the key derivation in slices, ChaCha20, HMAC;
//! - `export`: `ArchiveReader`, the archive as a `Read` (R4);
//! - `restore`: `restore` and `apply_pending` (R5, R6);
//! - `state`: the home's backup state and its ticket (R1, R4).
//!
//! The core holds no file, no clock and no sleep of its own: the files come through
//! `BackupFiles`, the clock and the pause through `Pacing`, the random bytes from the caller.

mod crypto;
mod export;
mod format;
mod restore;
mod state;

use std::io::{Read, Write};

pub use export::ArchiveReader;
pub use restore::apply_pending;
pub use state::{BackupState, BackupTicket, Phase, WriteMark};

use crate::api::{Response, error};

/// An export ends after this long from its first byte, in milliseconds, whatever the client does:
/// a client that stops reading cannot hold the Saver (R4). It is ten times the time the box needs
/// to send the largest archive over a slow link.
pub const MAX_EXPORT_MS: u64 = 120_000;
/// PBKDF2 iterations of a new archive: ten times the account's (D47), for a file that leaves the
/// box and can be attacked offline.
pub const BACKUP_ITERATIONS: u32 = 100_000;
/// The most iterations a restore accepts: four times the export's, which already holds an HTTP
/// worker for 5 to 10 s on the box; more would hold it for minutes.
pub const MAX_BACKUP_ITERATIONS: u32 = 400_000;
/// The longest archive a restore reads, in bytes: the worst-case home's files (every `MAX_*` of
/// the core reached) fit with room, measured by `tests/full_backup.rs`.
pub const MAX_ARCHIVE_BYTES: usize = 2 * 1024 * 1024;
/// The longest file of an archive, in bytes: the biggest file of the worst-case home
/// (`devices.json`, under 300 KB) with room.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
/// The most files of an archive: the seven names of `ARCHIVE_FILES`, each once.
pub const MAX_FILES: usize = 7;
/// The longest manifest, in bytes: seven entries of about 120 bytes, with room.
pub const MAX_MANIFEST_BYTES: usize = 4096;
/// The size of the one buffer an export or a restore holds, and the longest chunk of a file.
pub const CHUNK_BYTES: usize = 4096;

/// The shortest wait of an export for a file the Saver is writing, in milliseconds: the Saver
/// writes a file in well under this.
pub(crate) const WRITE_WAIT_MS: u64 = 5_000;
/// The key derivation hands the CPU back (the caller's pause) after this many iterations, so that
/// the box's task watchdog (5 s) is fed (R8).
pub const PAUSE_EVERY_ITERATIONS: u32 = 5_000;

/// The archive's name for the Zigbee network's file (the host keeps it as `network.json`).
pub const ZIGBEE_FILE: &str = "zigbee.json";
/// The archive's name for the Wi-Fi settings.
pub const WIFI_FILE: &str = "wifi.json";

/// One file of the archive: its name, and whether it is stored encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveFile {
    pub name: &'static str,
    pub encrypted: bool,
}

/// The files an archive may hold, in the order they are written (R2).
pub const ARCHIVE_FILES: [ArchiveFile; MAX_FILES] = [
    ArchiveFile {
        name: "devices.json",
        encrypted: false,
    },
    ArchiveFile {
        name: "zones.json",
        encrypted: false,
    },
    ArchiveFile {
        name: "equipments.json",
        encrypted: false,
    },
    ArchiveFile {
        name: "recipes.json",
        encrypted: false,
    },
    ArchiveFile {
        name: "settings.json",
        encrypted: false,
    },
    ArchiveFile {
        name: ZIGBEE_FILE,
        encrypted: true,
    },
    ArchiveFile {
        name: WIFI_FILE,
        encrypted: true,
    },
];

/// The archive file of this name, if it is one.
pub(crate) fn archive_file(name: &str) -> Option<ArchiveFile> {
    ARCHIVE_FILES.iter().copied().find(|f| f.name == name)
}

/// The clock and the pause the caller gives: the core has neither. The servers answer with the
/// monotonic time and a 10 ms sleep, the tests with a counter.
pub trait Pacing {
    /// A monotonic time, in milliseconds.
    fn now_ms(&mut self) -> u64;
    /// Hands the CPU back for a moment (feeds the watchdog; the wait for the Saver).
    fn pause(&mut self);
}

/// A staged copy of a file, being written: `finish` closes it and syncs it to the medium
/// (`sync_all`), so that the marker is only written once every copy is on the flash.
pub trait StagedFile: Write {
    fn finish(self: Box<Self>) -> Result<(), String>;
}

/// The files of the home and of the radio, behind the platform (R5, R6). Names are the archive's
/// (`ARCHIVE_FILES`): the host maps `zigbee.json` to its `network.json`. Errors are a message.
pub trait BackupFiles {
    /// True when the box has Wi-Fi settings (the Mac has none: `?wifi=true` is ignored there).
    fn has_wifi(&self) -> bool;
    /// True when the program restarts by itself after a restore (the box), false when the user
    /// restarts it (the Mac host): the answer's message says which.
    fn restarts(&self) -> bool;
    /// Opens the live file for reading; `None` when absent. When the live file is missing and
    /// its `<name>.new` is there (the window of a save, between the removal and the rename), it
    /// reads the `.new`, as `Storage::read` does.
    fn open(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String>;
    /// Creates (truncates) the staged copy `<name>.restore` and returns its writer.
    fn stage(&mut self, name: &str) -> Result<Box<dyn StagedFile>, String>;
    /// Opens the staged copy for reading; `None` when there is none.
    fn open_staged(&mut self, name: &str) -> Result<Option<Box<dyn Read>>, String>;
    /// Removes every staged copy (`<name>.restore`) and the marker `restore.pending`; never the
    /// one-shot mark `zigbee.restore`.
    fn discard(&mut self) -> Result<(), String>;
    /// Writes the marker atomically (temporary file, `.new`, rename).
    fn commit(&mut self, marker: &str) -> Result<(), String>;
    /// The marker's text, if any.
    fn pending(&mut self) -> Result<Option<String>, String>;
    /// Replaces the live file by its staged copy (FAT: the live file removed first, then the
    /// rename), and removes the name's leftover `.new` and `.tmp`. No-op when the copy is gone
    /// (an interrupted application finished): the leftovers are removed all the same.
    fn promote(&mut self, name: &str) -> Result<(), String>;
    /// Removes the live file and its leftover `.new` and `.tmp`. Absent: no-op.
    fn remove(&mut self, name: &str) -> Result<(), String>;
    /// Keeps the live `zigbee.json` of another network under a name of its PAN id
    /// (`zigbee-<label>.json`; the host `network-<label>.json`), moving it out of the way.
    fn set_aside(&mut self, name: &str, label: &str) -> Result<(), String>;
    /// Removes the marker.
    fn clear_pending(&mut self) -> Result<(), String>;
    /// True when the one-shot mark `zigbee.restore` is there (R7).
    fn restore_mark(&mut self) -> Result<bool, String>;
    /// Writes the mark `zigbee.restore`.
    fn set_restore_mark(&mut self) -> Result<(), String>;
    /// Removes the mark `zigbee.restore`, once the network is up.
    fn clear_restore_mark(&mut self) -> Result<(), String>;
}

/// The backup password from the header `X-Backup-Password`: percent-encoded UTF-8, used as the
/// bytes received, with no normalisation. `None` when absent, empty, badly encoded or not UTF-8.
pub fn password_from_header(value: Option<&str>) -> Option<String> {
    let bytes = value?.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let pair = bytes.get(i + 1..i + 3)?;
            let high = char::from(pair[0]).to_digit(16)?;
            let low = char::from(pair[1]).to_digit(16)?;
            out.push(u8::try_from(high * 16 + low).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(out).ok()?;
    (!text.is_empty()).then_some(text)
}

/// A refusal: a status and a message, the answer's `{"error": ...}`.
#[derive(Debug)]
pub(crate) struct Refusal {
    pub status: u16,
    pub message: String,
}

impl Refusal {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Refusal {
            status,
            message: message.into(),
        }
    }

    /// 400 `Invalid backup: <reason>`.
    pub fn invalid(reason: impl std::fmt::Display) -> Self {
        Refusal::new(400, format!("Invalid backup: {reason}"))
    }

    pub fn response(&self) -> Response {
        error(self.status, &self.message)
    }
}

pub(crate) fn password_required() -> Refusal {
    Refusal::new(400, "Backup password required")
}
