//! `Updates`: the key, the running version and the state of the installation, shared by the
//! program; `install` streams an update file into the inactive slot (R4), `watch` applies the
//! probation's verdict (R5).

use std::io::{ErrorKind, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ed25519_dalek::VerifyingKey;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    CHUNK_BYTES, HEADER_BYTES, Health, Probation, Refusal, UpdateAdapter, Verdict, Version,
    check_header, parse_public_key,
};
use crate::api::{Response, error};
use crate::devices::lock;

/// Where the installation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Installing,
    /// Written and made the next boot; the program waits for its restart.
    Installed,
}

/// The probation, as `watch` sees it.
enum Watch {
    /// `watch` has not run yet: the adapter is asked at its first tick.
    Unchecked,
    /// Not on probation, or the probation is over.
    Off,
    On(Probation),
}

/// What the rest of the program may read of the updates (spec 043): whether an installation runs
/// and whether the running image is on probation. Shared between `Updates` and the home, which
/// refuses a restart that would break either.
#[derive(Debug)]
pub struct UpdateStatus {
    state: Mutex<State>,
    on_probation: AtomicBool,
}

impl Default for UpdateStatus {
    fn default() -> Self {
        UpdateStatus {
            state: Mutex::new(State::Idle),
            on_probation: AtomicBool::new(false),
        }
    }
}

impl UpdateStatus {
    /// Why a restart now would do harm (`None`: it can go on): an installation in progress, or the
    /// running image on probation (a reset then makes the bootloader roll the update back). An
    /// update installed and waiting for its restart does not refuse: the box restarts anyway.
    pub fn restart_refusal(&self) -> Option<&'static str> {
        if matches!(*lock(&self.state), State::Installing) {
            return Some("Update already in progress");
        }
        if self.on_probation.load(Ordering::SeqCst) {
            return Some("The update is being checked, restart in a minute");
        }
        None
    }
}

/// The update machinery of a program: one per program, shared by its threads.
pub struct Updates {
    key: Option<VerifyingKey>,
    running: Version,
    status: Arc<UpdateStatus>,
    watch: Mutex<Watch>,
}

/// Puts the state back to idle on every path out of `install` (a panic included), unless the
/// installation went through.
pub(crate) struct Guard<'a> {
    updates: &'a Updates,
    installed: bool,
}

/// What a download promised, checked by the installation (spec 039, R4): the file's size (header
/// and image) and version, which the header must agree with before anything is written, and a
/// last check, run after the image and before the slot is made the next boot.
pub(crate) struct Expected<'a> {
    pub size: u64,
    pub version: Version,
    pub verify: &'a mut dyn FnMut() -> Result<(), Refusal>,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        *lock(&self.updates.status.state) = if self.installed {
            State::Installed
        } else {
            State::Idle
        };
    }
}

impl Updates {
    /// With the key of the build (`None`: updates are refused) and the running version.
    pub fn new(key: Option<VerifyingKey>, running: Version) -> Self {
        Updates {
            key,
            running,
            status: Arc::new(UpdateStatus::default()),
            watch: Mutex::new(Watch::Unchecked),
        }
    }

    /// From the build's key text (`option_env!`), for the running core's version: the updates and
    /// the line to log at start: `update: no key, updates refused`, `update: invalid key, updates
    /// refused` or `update: key <first 8 hex characters>`.
    pub fn from_key_text(text: Option<&str>) -> (Updates, String) {
        let running = Version::parse(crate::VERSION).unwrap_or(Version::new(0, 0, 0));
        let (key, line) = match text {
            None => (None, "update: no key, updates refused".to_string()),
            Some(text) => match parse_public_key(text) {
                None => (None, "update: invalid key, updates refused".to_string()),
                Some(key) => (
                    Some(key),
                    format!("update: key {}", text[..8].to_ascii_lowercase()),
                ),
            },
        };
        (Updates::new(key, running), line)
    }

    /// The status the home reads for the restart route (spec 043).
    pub fn status(&self) -> Arc<UpdateStatus> {
        Arc::clone(&self.status)
    }

    /// True when the build has a valid key.
    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }

    /// The running version.
    pub fn running(&self) -> Version {
        self.running
    }

    /// Installs the update file `source` yields into the inactive slot of `adapter` (R4): the
    /// answer is Sowel's `{"success": true, "message": ...}` or `{"error": ...}`. The image is
    /// never held whole: a header, one buffer of `CHUNK_BYTES` and the hash state.
    pub fn install(&self, source: &mut dyn Read, adapter: &mut dyn UpdateAdapter) -> Response {
        match self.claim() {
            Ok(guard) => self.install_claimed(guard, source, adapter, None),
            Err(refusal) => refuse(&refusal),
        }
    }

    /// Takes the installation: 503 without a key, 409 while another installation runs or one
    /// waits for the restart. The guard puts the state back when it is dropped.
    pub(crate) fn claim(&self) -> Result<Guard<'_>, Refusal> {
        if self.key.is_none() {
            return Err(Refusal::new(
                503,
                "Updates are not configured: this build has no update key",
            ));
        }
        let mut state = lock(&self.status.state);
        match *state {
            State::Installing => return Err(Refusal::new(409, "Update already in progress")),
            State::Installed => {
                return Err(Refusal::new(
                    409,
                    "An update is installed and waits for the restart",
                ));
            }
            State::Idle => {}
        }
        *state = State::Installing;
        Ok(Guard {
            updates: self,
            installed: false,
        })
    }

    /// Installs once the installation is claimed (`expected`: a download's promises, spec 039).
    pub(crate) fn install_claimed(
        &self,
        mut guard: Guard<'_>,
        source: &mut dyn Read,
        adapter: &mut dyn UpdateAdapter,
        expected: Option<Expected<'_>>,
    ) -> Response {
        let Some(key) = &self.key else {
            return refuse(&Refusal::new(
                503,
                "Updates are not configured: this build has no update key",
            ));
        };
        match self.run(key, source, adapter, expected) {
            Ok(done) => {
                guard.installed = true;
                done
            }
            Err(refusal) => refuse(&refusal),
        }
    }

    fn run(
        &self,
        key: &VerifyingKey,
        source: &mut dyn Read,
        adapter: &mut dyn UpdateAdapter,
        expected: Option<Expected<'_>>,
    ) -> Result<Response, Refusal> {
        if adapter.pending() {
            return Err(Refusal::new(409, "The running image is not confirmed yet"));
        }
        let mut header = [0u8; HEADER_BYTES];
        if read_full(source, &mut header) != Some(HEADER_BYTES) {
            return Err(Refusal::new(400, "Truncated update"));
        }
        let header = check_header(&header, key, self.running, adapter.capacity())?;
        let mut expected = expected;
        if let Some(expected) = &expected
            && (HEADER_BYTES as u64 + u64::from(header.length) != expected.size
                || header.version != expected.version)
        {
            return Err(mismatch());
        }
        adapter.begin(header.length).map_err(failed)?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; CHUNK_BYTES];
        let mut remaining = u64::from(header.length);
        while remaining > 0 {
            let want = usize::try_from(remaining).map_or(CHUNK_BYTES, |r| r.min(CHUNK_BYTES));
            if read_full(source, &mut buffer[..want]) != Some(want) {
                adapter.abort();
                return Err(Refusal::new(400, "Truncated update"));
            }
            hash.update(&buffer[..want]);
            if let Err(message) = adapter.write(&buffer[..want]) {
                adapter.abort();
                return Err(failed(message));
            }
            remaining -= want as u64;
        }
        // One byte more shows a file longer than declared; the source ends at its declared size.
        match read_full(source, &mut buffer[..1]) {
            Some(0) => {}
            Some(_) => {
                adapter.abort();
                return Err(Refusal::new(400, "Update longer than declared"));
            }
            None => {
                adapter.abort();
                return Err(Refusal::new(400, "Truncated update"));
            }
        }
        if hash.finalize().as_slice() != header.digest {
            adapter.abort();
            return Err(Refusal::new(400, "Image does not match its signature"));
        }
        if let Some(expected) = &mut expected
            && let Err(refusal) = (expected.verify)()
        {
            adapter.abort();
            return Err(refusal);
        }
        if let Err(message) = adapter.finish() {
            adapter.abort();
            return Err(failed(message));
        }
        log::info!(
            "update: {} installed, {} KB",
            header.version,
            header.length / 1024
        );
        Ok(Response {
            status: 200,
            body: Some(
                json!({
                    "success": true,
                    "message": format!("Installed {}, restarting", header.version),
                })
                .to_string(),
            ),
        })
    }

    /// Applies the probation's verdict once (R5), to be called about once a second with the
    /// monotonic time and the program's health. The first call asks `adapter.pending()`: a
    /// program not on probation then does nothing, ever. A `confirm` or `roll_back` that fails is
    /// logged and asked again at the next tick.
    pub fn watch(&self, adapter: &mut dyn UpdateAdapter, now_ms: u64, health: Health) {
        let mut watch = lock(&self.watch);
        if matches!(*watch, Watch::Unchecked) {
            *watch = if adapter.pending() {
                self.status.on_probation.store(true, Ordering::SeqCst);
                Watch::On(Probation::new(now_ms))
            } else {
                Watch::Off
            };
        }
        let Watch::On(probation) = &mut *watch else {
            return;
        };
        if let Some(reason) = probation.reason_changed(health) {
            log::warn!("update: unhealthy: {reason}");
        }
        match probation.tick(now_ms, health) {
            Verdict::Wait => {}
            Verdict::Confirm => match adapter.confirm() {
                Ok(()) => {
                    log::info!("update: running image confirmed");
                    *watch = Watch::Off;
                    self.status.on_probation.store(false, Ordering::SeqCst);
                }
                Err(message) => log::warn!("update: confirm failed: {message}"),
            },
            Verdict::RollBack => {
                log::warn!("update: not confirmed in time, rolling back");
                match adapter.roll_back() {
                    Ok(()) => {
                        *watch = Watch::Off;
                        self.status.on_probation.store(false, Ordering::SeqCst);
                    }
                    Err(message) => log::warn!("update: roll back failed: {message}"),
                }
            }
        }
    }
}

/// 400: the download is not the file the manifest announced.
pub(crate) fn mismatch() -> Refusal {
    Refusal::new(400, "Download does not match the manifest")
}

pub(crate) fn refuse(refusal: &Refusal) -> Response {
    error(refusal.status, &refusal.message)
}

/// A `begin`, `write` or `finish` error: 500, logged.
fn failed(message: String) -> Refusal {
    log::warn!("update: failed: {message}");
    Refusal::new(500, format!("Update failed: {message}"))
}

/// Reads until `buffer` is full or the source ends: the count read, or `None` on a read error
/// (a client gone, a timeout).
fn read_full(source: &mut dyn Read, buffer: &mut [u8]) -> Option<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match source.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    Some(filled)
}
