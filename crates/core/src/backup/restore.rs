//! The restore (R5, R6): the body read once through a 4 KB buffer into staged copies, everything
//! checked as it is read; only when the whole file holds is the marker written. The staged copies
//! replace the live files at the next start (`apply_pending`), before the home is loaded.

use std::io::{ErrorKind, Read};

use hmac::Mac;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::crypto::{HmacSha256, Keys, apply, derive_master, hmac};
use super::export::log_derivation;
use super::format::{
    FLAG_ENCRYPTED, FORMAT, HEADER_BYTES, Header, KIND_END, KIND_FILE, MAC_BYTES, MAGIC, Manifest,
    ManifestFile, PREFIX_BYTES,
};
use super::{
    ARCHIVE_FILES, BackupFiles, BackupTicket, CHUNK_BYTES, MAX_ARCHIVE_BYTES,
    MAX_BACKUP_ITERATIONS, MAX_FILE_BYTES, MAX_FILES, MAX_MANIFEST_BYTES, Pacing, Refusal,
    StagedFile, WIFI_FILE, ZIGBEE_FILE, archive_file, password_from_header, password_required,
};
use crate::api::Response;
use crate::auth::{MIN_ITERATIONS, ct_eq, hex};
use crate::radio::backup as radio_backup;

/// The last line of the marker: a marker without it was cut short and is not trusted.
const MARKER_END: &str = "end";

fn truncated() -> Refusal {
    Refusal::invalid("truncated")
}

fn too_large() -> Refusal {
    Refusal::new(413, "Backup too large")
}

/// A failure of the adapter: 500, logged.
fn failed(message: impl std::fmt::Display) -> Refusal {
    log::warn!("backup: restore failed: {message}");
    Refusal::new(500, format!("Restore failed: {message}"))
}

/// The body, counted against `MAX_ARCHIVE_BYTES`, and authenticated as it is read.
struct Wire<'a> {
    source: &'a mut dyn Read,
    mac: Option<HmacSha256>,
    count: usize,
}

impl Wire<'_> {
    /// Reads up to `buf.len()` bytes, until the body ends; the count read.
    fn read_up_to(&mut self, buf: &mut [u8]) -> Result<usize, Refusal> {
        let mut filled = 0;
        while filled < buf.len() {
            match self.source.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(count) => filled += count,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                // A client that went away: the body ends there.
                Err(_) => break,
            }
        }
        // The bytes actually read count, not the size asked for: an archive of exactly the bound is
        // accepted, even though the last probe asks for one byte past it.
        self.count += filled;
        if self.count > MAX_ARCHIVE_BYTES {
            return Err(too_large());
        }
        Ok(filled)
    }

    /// Reads exactly `buf.len()` bytes; a body that ends early is `truncated`. `authenticated`:
    /// the bytes are part of what the MAC covers.
    fn read_exact(&mut self, buf: &mut [u8], authenticated: bool) -> Result<(), Refusal> {
        if self.read_up_to(buf)? != buf.len() {
            return Err(truncated());
        }
        if authenticated && let Some(mac) = self.mac.as_mut() {
            mac.update(buf);
        }
        Ok(())
    }

    fn byte(&mut self) -> Result<u8, Refusal> {
        let mut one = [0u8; 1];
        self.read_exact(&mut one, true)?;
        Ok(one[0])
    }

    fn u16(&mut self) -> Result<u16, Refusal> {
        let mut two = [0u8; 2];
        self.read_exact(&mut two, true)?;
        Ok(u16::from_le_bytes(two))
    }
}

/// What a restore did.
struct Done {
    lines: usize,
    wifi: bool,
}

impl BackupTicket {
    /// Restores the archive of `source` (R5, R6): the declared length over `MAX_ARCHIVE_BYTES`
    /// is 413 before a byte is read, the password of the header `X-Backup-Password` missing is
    /// 400, then the body is read once, validated whole, staged, and committed. `wifi`: also
    /// restore the Wi-Fi settings (ignored when the adapter has none). The answer is Sowel's
    /// `{"success": true, "message", "files", "wifi"}` or `{"error"}`; after a refusal the body
    /// is not read to its end: the caller closes the connection. The ticket stays `pending` after
    /// a success until the restart.
    pub fn restore(
        mut self,
        wifi: bool,
        content_length: Option<u64>,
        password_header: Option<&str>,
        source: &mut dyn Read,
        files: &mut dyn BackupFiles,
        pacing: &mut dyn Pacing,
    ) -> Response {
        if content_length.is_some_and(|length| length > MAX_ARCHIVE_BYTES as u64) {
            return too_large().response();
        }
        let Some(password) = password_from_header(password_header) else {
            return password_required().response();
        };
        let mut staging = false;
        match run(
            &mut self,
            &password,
            wifi,
            source,
            files,
            pacing,
            &mut staging,
        ) {
            Ok(done) => {
                let message = if files.restarts() {
                    "Backup restored: the box restarts, log in again after it"
                } else {
                    "Backup restored: restart the host to load it"
                };
                Response {
                    status: 200,
                    body: Some(
                        json!({
                            "success": true,
                            "message": message,
                            "files": done.lines,
                            "wifi": done.wifi,
                        })
                        .to_string(),
                    ),
                }
            }
            Err(refusal) => {
                if staging && let Err(e) = files.discard() {
                    log::warn!("backup: cannot remove the staged copies: {e}");
                }
                refusal.response()
            }
        }
    }
}

/// A file name for a message: only if it is short printable ASCII.
fn shown(name: &[u8]) -> String {
    if !name.is_empty() && name.iter().all(u8::is_ascii_graphic) {
        String::from_utf8_lossy(name).into_owned()
    } else {
        "?".to_string()
    }
}

fn run(
    ticket: &mut BackupTicket,
    password: &str,
    wifi: bool,
    source: &mut dyn Read,
    files: &mut dyn BackupFiles,
    pacing: &mut dyn Pacing,
    staging: &mut bool,
) -> Result<Done, Refusal> {
    let mut wire = Wire {
        source,
        mac: None,
        count: 0,
    };
    // The magic and the format first: a newer file is refused before anything else is read.
    let mut header = [0u8; HEADER_BYTES];
    let got = wire.read_up_to(&mut header[..PREFIX_BYTES])?;
    let seen = got.min(MAGIC.len());
    if got == 0 || header[..seen] != MAGIC[..seen] {
        return Err(Refusal::invalid("not a Solite backup"));
    }
    if got < PREFIX_BYTES {
        return Err(truncated());
    }
    match u16::from_le_bytes([header[8], header[9]]) {
        FORMAT => {}
        newer if newer > FORMAT => {
            return Err(Refusal::new(
                400,
                "Backup made by a newer version of Solite",
            ));
        }
        other => return Err(Refusal::invalid(format!("unsupported format {other}"))),
    }
    wire.read_exact(&mut header[PREFIX_BYTES..], false)?;
    let parsed = Header::parse_rest(&header);
    if !(MIN_ITERATIONS..=MAX_BACKUP_ITERATIONS).contains(&parsed.iterations) {
        return Err(Refusal::invalid("iterations out of range"));
    }
    let derive_from = pacing.now_ms();
    let master = derive_master(
        password.as_bytes(),
        &parsed.salt,
        parsed.iterations,
        &mut || pacing.pause(),
    );
    log_derivation(
        pacing.now_ms().saturating_sub(derive_from),
        parsed.iterations,
    );
    let keys = Keys::new(&master);
    if !ct_eq(&keys.key_check(), &parsed.key_check) {
        return Err(Refusal::new(400, "Wrong backup password"));
    }
    let mut mac = hmac(&keys.authentication);
    mac.update(&header);
    wire.mac = Some(mac);

    files.discard().map_err(failed)?;
    *staging = true;
    let keep_wifi = wifi && files.has_wifi();
    // One buffer, on the heap: the chunk read, decrypted in place, written.
    let mut buffer = vec![0u8; CHUNK_BYTES];
    // At most `MAX_FILES`: each name once.
    let mut entries: Vec<ManifestFile> = Vec::new();
    let mut staged: Vec<&'static str> = Vec::new();
    loop {
        match wire.byte()? {
            KIND_END => break,
            KIND_FILE => {}
            other => return Err(Refusal::invalid(format!("unknown entry kind {other}"))),
        }
        if entries.len() >= MAX_FILES {
            return Err(Refusal::invalid("too many files"));
        }
        let name_len = usize::from(wire.byte()?);
        let mut name = [0u8; 32];
        if name_len > name.len() {
            return Err(Refusal::invalid("unknown file"));
        }
        wire.read_exact(&mut name[..name_len], true)?;
        let flags = wire.byte()?;
        let Some(file) = std::str::from_utf8(&name[..name_len])
            .ok()
            .and_then(archive_file)
        else {
            return Err(Refusal::invalid(format!(
                "unknown file {}",
                shown(&name[..name_len])
            )));
        };
        if entries.iter().any(|e| e.name == file.name) {
            return Err(Refusal::invalid(format!("{} twice", file.name)));
        }
        if flags != if file.encrypted { FLAG_ENCRYPTED } else { 0 } {
            return Err(Refusal::invalid(format!("{} stored wrongly", file.name)));
        }
        let mut writer: Option<Box<dyn StagedFile>> = if file.name != WIFI_FILE || keep_wifi {
            Some(files.stage(file.name).map_err(failed)?)
        } else {
            None
        };
        let index = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        let mut cipher = file.encrypted.then(|| keys.cipher(&parsed.nonce, index));
        let mut hash = Sha256::new();
        let mut size = 0u64;
        loop {
            let len = usize::from(wire.u16()?);
            if len == 0 {
                break;
            }
            if len > CHUNK_BYTES {
                return Err(Refusal::invalid("chunk too long"));
            }
            if size + len as u64 > MAX_FILE_BYTES as u64 {
                return Err(Refusal::invalid(format!("{} too large", file.name)));
            }
            wire.read_exact(&mut buffer[..len], true)?;
            // The hash and the MAC are of the ciphertext, as stored.
            hash.update(&buffer[..len]);
            size += len as u64;
            if let Some(cipher) = cipher.as_mut() {
                apply(cipher, &mut buffer[..len]);
            }
            if let Some(writer) = writer.as_mut() {
                writer.write_all(&buffer[..len]).map_err(failed)?;
            }
        }
        if let Some(writer) = writer.take() {
            // Closed and synced: on the flash before the marker is written.
            writer.finish().map_err(failed)?;
            staged.push(file.name);
        }
        entries.push(ManifestFile {
            name: file.name.to_string(),
            size,
            sha256: hex(&hash.finalize()),
            encrypted: file.encrypted,
        });
    }

    let manifest_len = usize::from(wire.u16()?);
    if manifest_len == 0 || manifest_len > MAX_MANIFEST_BYTES {
        return Err(Refusal::invalid("manifest too long"));
    }
    let mut manifest = vec![0u8; manifest_len];
    wire.read_exact(&mut manifest, true)?;
    let expected = wire
        .mac
        .take()
        .map(|mac| mac.finalize().into_bytes())
        .ok_or_else(|| failed("no MAC"))?;
    let mut tag = [0u8; MAC_BYTES];
    wire.read_exact(&mut tag, false)?;
    if !ct_eq(&expected, &tag) {
        return Err(Refusal::invalid("the file is damaged or was changed"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&manifest).map_err(|_| Refusal::invalid("unreadable manifest"))?;
    if manifest.version != FORMAT || manifest.files != entries {
        return Err(Refusal::invalid("the manifest does not match the files"));
    }
    // Nothing after the MAC.
    let mut more = [0u8; 1];
    if wire.read_up_to(&mut more)? != 0 {
        return Err(Refusal::invalid("data after the end"));
    }

    // Everything holds: the marker names what the next start does, in the order of the archive.
    let mut marker = String::new();
    // `files` counts the files restored: the removals of the home's own files are not counted.
    let lines = staged.len();
    for name in &staged {
        marker.push_str(&format!("promote {name}\n"));
    }
    // A home file the archive does not have is removed: the home is the archive's.
    for file in ARCHIVE_FILES.iter().filter(|f| !f.encrypted) {
        if !entries.iter().any(|e| e.name == file.name) {
            marker.push_str(&format!("remove {}\n", file.name));
        }
    }
    marker.push_str(MARKER_END);
    marker.push('\n');
    files.commit(&marker).map_err(failed)?;
    ticket.set_pending();
    Ok(Done {
        lines,
        wifi: staged.contains(&WIFI_FILE),
    })
}

/// One line of the marker.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Promote(&'static str),
    Remove(&'static str),
}

/// The marker's steps, or `None` when it does not parse: a line that is not `promote <name>` or
/// `remove <name>` with a name of the archive, or no `end` line (a marker cut short).
fn parse_marker(text: &str) -> Option<Vec<Step>> {
    let mut steps = Vec::new();
    let mut lines = text.lines();
    loop {
        let line = lines.next()?;
        if line == MARKER_END {
            return lines.next().is_none().then_some(steps);
        }
        let (verb, name) = line.split_once(' ')?;
        let file = archive_file(name)?;
        steps.push(match verb {
            "promote" => Step::Promote(file.name),
            "remove" => Step::Remove(file.name),
            _ => return None,
        });
        // At most one step per file.
        if steps.len() > MAX_FILES {
            return None;
        }
    }
}

fn read_text(reader: Box<dyn Read>) -> Option<String> {
    let mut text = String::new();
    // Bounded: a file of the archive is at most `MAX_FILE_BYTES`.
    reader
        .take(MAX_FILE_BYTES as u64)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Before `zigbee.json` is promoted: the live network, if it is another one, is kept under its
/// PAN id (the keys of the radio's current network are never lost), and the one-shot mark that
/// asks the network start to leave it is written (R7). Nothing when the staged copy is gone (an
/// interrupted application finished).
fn prepare_zigbee(files: &mut dyn BackupFiles) -> Result<(), String> {
    if files.open_staged(ZIGBEE_FILE)?.is_none() {
        return Ok(());
    }
    let live = files.open(ZIGBEE_FILE)?.and_then(read_text);
    let staged = files.open_staged(ZIGBEE_FILE)?.and_then(read_text);
    if let Some(live) = live {
        let same = match (radio_backup::parse(&live), staged.as_deref()) {
            (Some(a), Some(staged)) => radio_backup::parse(staged)
                .is_some_and(|b| a.pan_id == b.pan_id && a.extended_pan_id == b.extended_pan_id),
            _ => false,
        };
        if !same {
            files.set_aside(ZIGBEE_FILE, &radio_backup::pan_label(&live))?;
        }
    }
    files.set_restore_mark()
}

/// Applies a committed restore (R6), first thing at every start, before anything reads the data
/// folder: the staged copies replace the live files and the home files the archive lacked are
/// removed, then the marker goes. An interruption is finished by the next call: the marker still
/// names what is left. A staged copy without a marker (a refused or interrupted restore) is
/// removed; a marker that does not parse is discarded whole, with its copies. The number of files
/// restored (promoted; the removals are not counted).
pub fn apply_pending(files: &mut dyn BackupFiles) -> Result<usize, String> {
    let Some(text) = files.pending()? else {
        files.discard()?;
        return Ok(0);
    };
    let Some(steps) = parse_marker(&text) else {
        log::warn!("backup: the restore marker is not valid, the staged copies are removed");
        files.discard()?;
        return Ok(0);
    };
    for step in &steps {
        match step {
            Step::Promote(name) => {
                if *name == ZIGBEE_FILE {
                    prepare_zigbee(files)?;
                }
                files.promote(name)?;
            }
            Step::Remove(name) => files.remove(name)?,
        }
    }
    files.clear_pending()?;
    let restored = steps
        .iter()
        .filter(|step| matches!(step, Step::Promote(_)))
        .count();
    log::info!("backup: {restored} files restored");
    Ok(restored)
}
