//! `Updates`: the key, the running version and the state of the installation, shared by the
//! program; `install` streams an update file into the inactive slot (R4), `watch` applies the
//! probation's verdict (R5).

use std::io::{ErrorKind, Read};
use std::sync::Mutex;

use ed25519_dalek::VerifyingKey;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{
    CHUNK_BYTES, HEADER_BYTES, Probation, Refusal, UpdateAdapter, Verdict, Version, check_header,
    parse_public_key,
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

/// The update machinery of a program: one per program, shared by its threads.
pub struct Updates {
    key: Option<VerifyingKey>,
    running: Version,
    state: Mutex<State>,
    watch: Mutex<Watch>,
}

/// Puts the state back to idle on every path out of `install` (a panic included), unless the
/// installation went through.
struct Guard<'a> {
    updates: &'a Updates,
    installed: bool,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        *lock(&self.updates.state) = if self.installed {
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
            state: Mutex::new(State::Idle),
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

    /// True when the build has a valid key.
    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }

    /// Installs the update file `source` yields into the inactive slot of `adapter` (R4): the
    /// answer is Sowel's `{"success": true, "message": ...}` or `{"error": ...}`. The image is
    /// never held whole: a header, one buffer of `CHUNK_BYTES` and the hash state.
    pub fn install(&self, source: &mut dyn Read, adapter: &mut dyn UpdateAdapter) -> Response {
        let Some(key) = &self.key else {
            return refuse(&Refusal::new(
                503,
                "Updates are not configured: this build has no update key",
            ));
        };
        let mut guard = {
            let mut state = lock(&self.state);
            match *state {
                State::Installing => {
                    return refuse(&Refusal::new(409, "Update already in progress"));
                }
                State::Installed => {
                    return refuse(&Refusal::new(
                        409,
                        "An update is installed and waits for the restart",
                    ));
                }
                State::Idle => {}
            }
            *state = State::Installing;
            Guard {
                updates: self,
                installed: false,
            }
        };
        match self.run(key, source, adapter) {
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
    ) -> Result<Response, Refusal> {
        if adapter.pending() {
            return Err(Refusal::new(409, "The running image is not confirmed yet"));
        }
        let mut header = [0u8; HEADER_BYTES];
        if read_full(source, &mut header) != Some(HEADER_BYTES) {
            return Err(Refusal::new(400, "Truncated update"));
        }
        let header = check_header(&header, key, self.running, adapter.capacity())?;
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
    pub fn watch(&self, adapter: &mut dyn UpdateAdapter, now_ms: u64, healthy: bool) {
        let mut watch = lock(&self.watch);
        if matches!(*watch, Watch::Unchecked) {
            *watch = if adapter.pending() {
                Watch::On(Probation::new(now_ms))
            } else {
                Watch::Off
            };
        }
        let Watch::On(probation) = &mut *watch else {
            return;
        };
        match probation.tick(now_ms, healthy) {
            Verdict::Wait => {}
            Verdict::Confirm => match adapter.confirm() {
                Ok(()) => {
                    log::info!("update: running image confirmed");
                    *watch = Watch::Off;
                }
                Err(message) => log::warn!("update: confirm failed: {message}"),
            },
            Verdict::RollBack => {
                log::warn!("update: not confirmed in time, rolling back");
                match adapter.roll_back() {
                    Ok(()) => *watch = Watch::Off,
                    Err(message) => log::warn!("update: roll back failed: {message}"),
                }
            }
        }
    }
}

fn refuse(refusal: &Refusal) -> Response {
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
