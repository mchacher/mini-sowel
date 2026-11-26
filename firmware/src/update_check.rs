//! The update's stack check of the image QEMU runs (feature `sim`, D60): the core's whole
//! installation (spec 032), with a key pair made at boot from the ESP32's random numbers (no key in
//! any file) and an image of 64 KB in memory, run on the very thread the radio image runs it on
//! (`job_thread::run`, `UPDATE_STACK_BYTES`). Three files: valid (200), one image byte changed
//! (400) and one signature byte changed (400). The thread logs `update: thread stack <n> of <m>
//! bytes never used`, which `scripts/qemu.sh` reads: an overflow restarts the chip, and fewer than
//! 2 KB unused fails the run. On the box, the API workers' 10 KB stacks overflowed in the signature
//! check (2026-11-25).
//!
//! Then one pull of a release (spec 039 R10), from an in-memory fetcher (a manifest, one redirect,
//! a signed file made at boot) on an `update-pull` thread of `FETCH_STACK_BYTES`, the very thread
//! the radio image runs it on: the core's `ReleaseTicket::pull` (the manifest's rules, the redirect,
//! the download into the installation). It logs `update pull check: status <s>`, and the thread
//! `update-pull: thread stack <n> of 20480 bytes never used`, which `scripts/qemu.sh` reads: it
//! fails when fewer than 8192 bytes stay unused, the room left for the TLS handshake, which QEMU
//! cannot run (this image has no network, no `fetch.rs`).

use ed25519_dalek::{Signer, SigningKey};
use esp_idf_svc::sys::esp_fill_random;
use solite_core::backup::Pacing;
use solite_core::home::CLOCK_SET_AFTER_MS;
use solite_core::update::release::{Fetched, Fetcher, Limits, Releases};
use solite_core::update::{UpdateAdapter, Updates, Version, image_digest, signed_bytes};

use crate::job_thread::{self, FETCH_STACK_BYTES, UPDATE_STACK_BYTES};

/// The image's size: a few chunks, the hash's loop runs.
const IMAGE_BYTES: usize = 64 * 1024;

/// The inactive slot as a count of bytes: the flash is not the subject.
struct Counted(usize);

impl UpdateAdapter for Counted {
    fn capacity(&self) -> u64 {
        solite_core::update::SLOT_BYTES
    }
    fn begin(&mut self, _length: u32) -> Result<(), String> {
        self.0 = 0;
        Ok(())
    }
    fn write(&mut self, chunk: &[u8]) -> Result<(), String> {
        self.0 += chunk.len();
        Ok(())
    }
    fn abort(&mut self) {}
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn pending(&self) -> bool {
        false
    }
    fn confirm(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn roll_back(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// An update file: the header signed with `key`, then `image`.
fn update_file(key: &SigningKey, image: &[u8]) -> Vec<u8> {
    let length = u32::try_from(image.len()).unwrap_or(u32::MAX);
    let signed = signed_bytes(Version::new(9, 0, 0), length, &image_digest(image));
    let mut file = signed.to_vec();
    file.extend_from_slice(&key.sign(&signed).to_bytes());
    file.extend_from_slice(image);
    file
}

/// The status of one installation of `file`, with a fresh `Updates` (each is idle).
fn install(public_hex: &str, file: &[u8]) -> u16 {
    let (updates, _) = Updates::from_key_text(Some(public_hex));
    let mut source = file;
    updates.install(&mut source, &mut Counted(0)).status
}

/// The address of the manifest, of the file (which answers a redirect) and of the file's new place.
const MANIFEST_URL: &str = "https://releases.example.test/manifest.json";
const FILE_URL: &str = "https://releases.example.test/solite.sup";
const FILE_MOVED_URL: &str = "https://objects.example.test/solite.sup";

/// The fetch adapter of the check: a manifest, one redirect, the signed file, all in memory.
struct InMemory {
    manifest: Vec<u8>,
    file: Vec<u8>,
}

impl Fetcher for InMemory {
    fn get(&mut self, url: &str, _idle_ms: u64) -> Result<Fetched, String> {
        let body = |bytes: &[u8]| Fetched::Body {
            length: Some(bytes.len() as u64),
            body: Box::new(std::io::Cursor::new(bytes.to_vec())),
        };
        match url {
            MANIFEST_URL => Ok(body(&self.manifest)),
            FILE_URL => Ok(Fetched::Redirect(Some(FILE_MOVED_URL.to_string()))),
            FILE_MOVED_URL => Ok(body(&self.file)),
            _ => Ok(Fetched::Status(404)),
        }
    }
}

/// The clock and the pause, on std (the host's and the box's kind).
struct Clock(std::time::Instant);

impl Pacing for Clock {
    fn now_ms(&mut self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    fn pause(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// The status of one pull of `file`, on the current thread: the manifest names `file` (its size
/// and digest), version 9.0.0.
fn pull(public_hex: &str, file: &[u8]) -> Option<u16> {
    let digest: String = image_digest(file)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let manifest = format!(
        r#"{{"format":1,"version":"9.0.0","minVersion":null,"image":{{"size":{},"sha256":"{digest}","url":"{FILE_URL}"}}}}"#,
        file.len()
    );
    let (updates, _) = Updates::from_key_text(Some(public_hex));
    let releases = std::sync::Arc::new(Releases::default());
    let ticket = releases.begin_pull(MANIFEST_URL, CLOCK_SET_AFTER_MS).ok()?;
    let mut fetcher = InMemory {
        manifest: manifest.into_bytes(),
        file: file.to_vec(),
    };
    let mut clock = Clock(std::time::Instant::now());
    Some(
        ticket
            .pull(
                &updates,
                &mut fetcher,
                &mut Counted(0),
                &mut clock,
                Limits::CHECK,
                Limits::DOWNLOAD,
            )
            .status,
    )
}

/// Runs the check and logs `update check: valid <s>, image changed <s>, signature changed <s>`.
#[allow(unsafe_code)]
pub fn run() {
    let mut seed = [0u8; 32];
    // SAFETY: `seed` is valid for writes of its length for the duration of the call.
    unsafe { esp_fill_random(seed.as_mut_ptr().cast(), seed.len()) };
    let key = SigningKey::from_bytes(&seed);
    let public_hex: String = key
        .verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let image: Vec<u8> = (0..IMAGE_BYTES).map(|i| (i * 7 % 251) as u8).collect();
    let valid = update_file(&key, &image);
    let mut image_changed = valid.clone();
    image_changed[128 + IMAGE_BYTES / 2] ^= 1;
    let mut signature_changed = valid.clone();
    signature_changed[100] ^= 1;
    let ran = job_thread::run(c"update", UPDATE_STACK_BYTES, || {
        [
            install(&public_hex, &valid),
            install(&public_hex, &image_changed),
            install(&public_hex, &signature_changed),
        ]
    });
    match ran {
        Ok(Some([valid, image, signature])) => log::info!(
            "update check: valid {valid}, image changed {image}, signature changed {signature}"
        ),
        Ok(None) => log::error!("update check: its thread panicked"),
        Err(e) => log::error!("update check: {e}"),
    }
    let pulled = job_thread::run(c"update-pull", FETCH_STACK_BYTES, || {
        pull(&public_hex, &valid)
    });
    match pulled {
        Ok(Some(Some(status))) => log::info!("update pull check: status {status}"),
        Ok(Some(None)) => log::error!("update pull check: no ticket"),
        Ok(None) => log::error!("update pull check: its thread panicked"),
        Err(e) => log::error!("update pull check: {e}"),
    }
}
