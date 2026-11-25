//! The export (R4): the archive as a `Read`, built on the fly from the files as last saved. The
//! archive is never held whole: one buffer of `CHUNK_BYTES`, the hash, MAC and cipher states.

use std::io::{self, ErrorKind, Read};

use chacha20::ChaCha20;
use hmac::Mac;
use sha2::{Digest, Sha256};

use super::crypto::{HmacSha256, Keys, apply, derive_master, hmac};
use super::format::{
    FLAG_ENCRYPTED, Header, KIND_END, KIND_FILE, MAC_BYTES, Manifest, ManifestFile,
};
use super::{
    ARCHIVE_FILES, BACKUP_ITERATIONS, BackupFiles, BackupTicket, CHUNK_BYTES, MAX_EXPORT_MS,
    MAX_FILE_BYTES, MAX_MANIFEST_BYTES, Pacing, Refusal, WRITE_WAIT_MS, password_from_header,
    password_required,
};
use crate::api::Response;
use crate::auth::{MAX_PASSWORD_CHARS, MIN_PASSWORD_CHARS, hex};
use crate::devices::Random;

/// The file being read.
struct Current {
    reader: Box<dyn Read>,
    name: &'static str,
    cipher: Option<ChaCha20>,
    hash: Sha256,
    size: u64,
}

enum Stage {
    Files,
    Finished,
    Failed,
}

/// The archive, as a stream of bytes. A failure while reading (a file that cannot be read, the
/// time limit) ends the stream with an error and without its end and MAC: the client keeps a
/// truncated file that a restore refuses.
pub struct ArchiveReader {
    files: Box<dyn BackupFiles>,
    pacing: Box<dyn Pacing>,
    /// Held until the end of the stream, so that the state goes back to idle at once.
    ticket: Option<BackupTicket>,
    mac: HmacSha256,
    keys: Keys,
    nonce: [u8; 8],
    /// The bytes ready to send: at most one chunk with its framing (the one buffer).
    out: Vec<u8>,
    pos: usize,
    stage: Stage,
    next_file: usize,
    /// At most `MAX_FILES`.
    entries: Vec<ManifestFile>,
    current: Option<Current>,
    started: Option<u64>,
}

impl BackupTicket {
    /// Starts the export (R1, R4): the password of the header `X-Backup-Password` checked (400),
    /// the key derived (estimated at 11 to 14 s on the box, logged `backup: key derived`, the lock
    /// released, `pacing.pause` every 5,000 iterations), the Saver's write waited for (503 after
    /// 5 s), then the stream. The errors are
    /// whole answers, to send before the status line of the stream. The ticket ends with the
    /// stream (or with the error).
    pub fn export(
        self,
        password_header: Option<&str>,
        files: Box<dyn BackupFiles>,
        pacing: Box<dyn Pacing>,
        random: Random,
    ) -> Result<ArchiveReader, Response> {
        self.export_with_iterations(password_header, files, pacing, random, BACKUP_ITERATIONS)
    }

    /// `export` with another iteration count: for the tests, which would else spend their time
    /// deriving keys. Not part of the API of the tool.
    #[doc(hidden)]
    pub fn export_with_iterations(
        self,
        password_header: Option<&str>,
        files: Box<dyn BackupFiles>,
        mut pacing: Box<dyn Pacing>,
        random: Random,
        iterations: u32,
    ) -> Result<ArchiveReader, Response> {
        let password = password_from_header(password_header).ok_or_else(|| {
            let refusal = password_required();
            refusal.response()
        })?;
        let chars = password.chars().count();
        if !(MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&chars) {
            return Err(
                Refusal::new(400, "Backup password must be 8 to 128 characters").response(),
            );
        }
        let mut salt = [0u8; 16];
        let mut nonce = [0u8; 8];
        if let Err(e) = random(&mut salt).and_then(|()| random(&mut nonce)) {
            log::error!("backup: no random bytes: {e}");
            return Err(Refusal::new(500, "Internal error").response());
        }
        let derive_from = pacing.now_ms();
        let master = derive_master(password.as_bytes(), &salt, iterations, &mut || {
            pacing.pause()
        });
        log_derivation(pacing.now_ms().saturating_sub(derive_from), iterations);
        let keys = Keys::new(&master);
        // The Saver may be writing a file: wait for it to end, then switch to `exporting` in the
        // same critical section as the check.
        let waited_from = pacing.now_ms();
        while !self.hold_for_export() {
            if pacing.now_ms().saturating_sub(waited_from) >= WRITE_WAIT_MS {
                return Err(Refusal::new(503, "Busy saving, try again").response());
            }
            pacing.pause();
        }
        let header = Header {
            iterations,
            salt,
            nonce,
            key_check: keys.key_check(),
        }
        .encode();
        let mut mac = hmac(&keys.authentication);
        mac.update(&header);
        let mut out = Vec::with_capacity(2 + CHUNK_BYTES);
        out.extend_from_slice(&header);
        Ok(ArchiveReader {
            files,
            pacing,
            ticket: Some(self),
            mac,
            keys,
            nonce,
            out,
            pos: 0,
            stage: Stage::Files,
            next_file: 0,
            entries: Vec::new(),
            current: None,
            started: None,
        })
    }
}

/// The `backup: key derived` line, the measure of the derivation on the box (export and restore).
pub(crate) fn log_derivation(ms: u64, iterations: u32) {
    log::info!("backup: key derived in {ms} ms ({iterations} iterations)");
}

fn other(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

impl ArchiveReader {
    /// Ends the job: the ticket is dropped (the state goes back to idle), the file is closed.
    fn release(&mut self) {
        self.ticket = None;
        self.current = None;
    }

    fn fail(&mut self) {
        self.stage = Stage::Failed;
        self.release();
    }

    /// Prepares the next bytes in `out`.
    fn refill(&mut self) -> io::Result<()> {
        self.out.clear();
        self.pos = 0;
        if self.current.is_some() {
            return self.next_chunk();
        }
        while self.next_file < ARCHIVE_FILES.len() {
            let file = ARCHIVE_FILES[self.next_file];
            self.next_file += 1;
            let reader = self
                .files
                .open(file.name)
                .map_err(|e| other(format!("{}: {e}", file.name)))?;
            let Some(reader) = reader else {
                continue;
            };
            let index = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
            self.out.push(KIND_FILE);
            // The names are ASCII and short.
            self.out.push(u8::try_from(file.name.len()).unwrap_or(0));
            self.out.extend_from_slice(file.name.as_bytes());
            self.out
                .push(if file.encrypted { FLAG_ENCRYPTED } else { 0 });
            self.mac.update(&self.out);
            self.current = Some(Current {
                reader,
                name: file.name,
                cipher: file.encrypted.then(|| self.keys.cipher(&self.nonce, index)),
                hash: Sha256::new(),
                size: 0,
            });
            return Ok(());
        }
        self.finish()
    }

    /// The next chunk of the current file, or the end of the file.
    fn next_chunk(&mut self) -> io::Result<()> {
        let Some(current) = self.current.as_mut() else {
            return Ok(());
        };
        self.out.resize(2 + CHUNK_BYTES, 0);
        let mut filled = 0;
        while filled < CHUNK_BYTES {
            match current.reader.read(&mut self.out[2 + filled..]) {
                Ok(0) => break,
                Ok(count) => filled += count,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(other(format!("{}: {e}", current.name))),
            }
        }
        self.out.truncate(2 + filled);
        if filled == 0 {
            // The end of the file: a chunk of length 0, and the file's line of the manifest.
            self.out[..2].copy_from_slice(&0u16.to_le_bytes());
            self.mac.update(&self.out);
            if let Some(done) = self.current.take() {
                self.entries.push(ManifestFile {
                    name: done.name.to_string(),
                    size: done.size,
                    sha256: hex(&done.hash.finalize()),
                    encrypted: done.cipher.is_some(),
                });
            }
            return Ok(());
        }
        let len = u16::try_from(filled).map_err(|_| other("chunk too long"))?;
        if current.size + u64::from(len) > MAX_FILE_BYTES as u64 {
            return Err(other(format!("{}: too large for a backup", current.name)));
        }
        if let Some(cipher) = current.cipher.as_mut() {
            apply(cipher, &mut self.out[2..]);
        }
        // The manifest holds what is stored: the ciphertext's hash.
        current.hash.update(&self.out[2..]);
        current.size += u64::from(len);
        self.out[..2].copy_from_slice(&len.to_le_bytes());
        self.mac.update(&self.out);
        Ok(())
    }

    /// The end: kind 0, the manifest, the MAC. Every file was read: the job ends.
    fn finish(&mut self) -> io::Result<()> {
        let manifest = Manifest::new(std::mem::take(&mut self.entries)).to_bytes();
        let len = u16::try_from(manifest.len())
            .ok()
            .filter(|len| usize::from(*len) <= MAX_MANIFEST_BYTES && *len > 0)
            .ok_or_else(|| other("manifest too long"))?;
        self.out.push(KIND_END);
        self.out.extend_from_slice(&len.to_le_bytes());
        self.out.extend_from_slice(&manifest);
        self.mac.update(&self.out);
        let tag: [u8; MAC_BYTES] = self.mac.clone().finalize().into_bytes().into();
        self.out.extend_from_slice(&tag);
        self.stage = Stage::Finished;
        self.release();
        Ok(())
    }
}

impl Read for ArchiveReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let now = self.pacing.now_ms();
        match self.started {
            None => self.started = Some(now),
            Some(started) if now.saturating_sub(started) >= MAX_EXPORT_MS => {
                if !matches!(self.stage, Stage::Finished) {
                    self.fail();
                    return Err(io::Error::new(ErrorKind::TimedOut, "export took too long"));
                }
            }
            Some(_) => {}
        }
        loop {
            if self.pos < self.out.len() {
                let count = buf.len().min(self.out.len() - self.pos);
                buf[..count].copy_from_slice(&self.out[self.pos..self.pos + count]);
                self.pos += count;
                return Ok(count);
            }
            match self.stage {
                Stage::Finished => return Ok(0),
                Stage::Failed => return Err(other("export failed")),
                Stage::Files => {}
            }
            if let Err(e) = self.refill() {
                log::warn!("backup: export failed: {e}");
                self.fail();
                return Err(e);
            }
        }
    }
}
