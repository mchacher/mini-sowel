//! The update's stack check of the image QEMU runs (feature `sim`, D60): the core's whole
//! installation (spec 032), with a key pair made at boot from the ESP32's random numbers (no key in
//! any file) and an image of 64 KB in memory, run on the very thread the radio image runs it on
//! (`job_thread::run`, `UPDATE_STACK_BYTES`). Three files: valid (200), one image byte changed
//! (400) and one signature byte changed (400). The thread logs `update: thread stack <n> of <m>
//! bytes never used`, which `scripts/qemu.sh` reads: an overflow restarts the chip, and fewer than
//! 2 KB unused fails the run. On the box, the API workers' 10 KB stacks overflowed in the signature
//! check (2026-11-25).

use ed25519_dalek::{Signer, SigningKey};
use esp_idf_svc::sys::esp_fill_random;
use solite_core::update::{UpdateAdapter, Updates, Version, image_digest, signed_bytes};

use crate::job_thread::{self, UPDATE_STACK_BYTES};

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
}
