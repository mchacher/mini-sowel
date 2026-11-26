//! Signed updates (spec 032, D52 to D54): the update file's header and its checks, the adapter
//! trait of the inactive slot, the streamed installation and the probation of a new image.
//!
//! The update file is a header of 128 bytes (magic, format, version, image length, SHA-256 of the
//! image, an Ed25519 signature over its first 64 bytes), then the image. The core verifies, with
//! the public key of the build, and never reads the environment or the flash: the host (files) and
//! the box (ESP-IDF's OTA) implement `UpdateAdapter`.

mod install;
mod probation;
pub mod release;

pub use install::{UpdateStatus, Updates};
pub use probation::{Health, Probation, Verdict};

use std::fmt;

use ed25519_dalek::{Signature, VerifyingKey};

/// The header's size.
pub const HEADER_BYTES: usize = 128;
/// The installation's one buffer: a chunk is at most this long (the bound of its memory).
pub const CHUNK_BYTES: usize = 4096;
/// Healthy for this long without a break confirms the running image (ms).
pub const CONFIRM_AFTER_MS: u64 = 60_000;
/// Not confirmed this long after the start, the running image is rolled back (ms).
pub const ROLLBACK_AFTER_MS: u64 = 300_000;
/// The inactive slot's size of the box (and of the host's files), in bytes.
pub const SLOT_BYTES: u64 = 0x40_0000;
/// The most an upload can declare: the header, a slot's worth of image, and one byte more (the
/// byte that shows an image longer than declared).
pub const MAX_UPLOAD_BYTES: u64 = HEADER_BYTES as u64 + SLOT_BYTES + 1;
/// The content type of the upload.
pub const CONTENT_TYPE: &str = "application/octet-stream";

const MAGIC: &[u8; 8] = b"SOLITEUP";
const FORMAT: u32 = 1;
/// The bytes the signature covers: the header without the signature.
const SIGNED_LEN: usize = 64;

/// `major.minor.patch`, each a `u16`; ordered as a tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Version {
            major,
            minor,
            patch,
        }
    }

    /// Exactly three numbers of digits only, separated by dots, each at most 65535.
    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.split('.');
        let mut next = || {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse::<u16>().ok()
        };
        let version = Version::new(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What a header says, once checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: Version,
    /// The image's size in bytes.
    pub length: u32,
    /// SHA-256 of the image.
    pub digest: [u8; 32],
}

/// The 64 bytes the signature covers (header bytes 0 to 63), for the host's `--update-sign`.
pub fn signed_bytes(version: Version, length: u32, digest: &[u8; 32]) -> [u8; SIGNED_LEN] {
    let mut out = [0u8; SIGNED_LEN];
    out[..8].copy_from_slice(MAGIC);
    out[8..12].copy_from_slice(&FORMAT.to_le_bytes());
    out[12..14].copy_from_slice(&version.major.to_le_bytes());
    out[14..16].copy_from_slice(&version.minor.to_le_bytes());
    out[16..18].copy_from_slice(&version.patch.to_le_bytes());
    out[20..24].copy_from_slice(&length.to_le_bytes());
    out[24..56].copy_from_slice(digest);
    out
}

/// SHA-256 of an image, as the header carries it (for the host's `--update-sign`).
pub fn image_digest(image: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(image).into()
}

/// Why a header or an installation is refused: the status and the message of the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub status: u16,
    pub message: String,
}

impl Refusal {
    pub(crate) fn new(status: u16, message: impl Into<String>) -> Self {
        Refusal {
            status,
            message: message.into(),
        }
    }
}

fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Checks a header, in the order of R1, before anything is written: magic, format, reserved
/// bytes and length, signature (`verify_strict`), version newer than `running`, length within
/// `capacity`.
pub fn check_header(
    bytes: &[u8; HEADER_BYTES],
    key: &VerifyingKey,
    running: Version,
    capacity: u64,
) -> Result<Header, Refusal> {
    if &bytes[..8] != MAGIC {
        return Err(Refusal::new(400, "Not a Solite update"));
    }
    let format = le32(&bytes[8..12]);
    if format != FORMAT {
        return Err(Refusal::new(
            400,
            format!("Unsupported update format {format}"),
        ));
    }
    let length = le32(&bytes[20..24]);
    if le16(&bytes[18..20]) != 0 || bytes[56..64].iter().any(|b| *b != 0) || length == 0 {
        return Err(Refusal::new(400, "Malformed update header"));
    }
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&bytes[64..]);
    if key
        .verify_strict(&bytes[..SIGNED_LEN], &Signature::from_bytes(&signature))
        .is_err()
    {
        return Err(Refusal::new(400, "Invalid signature"));
    }
    let version = Version::new(
        le16(&bytes[12..14]),
        le16(&bytes[14..16]),
        le16(&bytes[16..18]),
    );
    if version <= running {
        return Err(Refusal::new(
            400,
            format!("Version {version} is not newer than {running}"),
        ));
    }
    if u64::from(length) > capacity {
        return Err(Refusal::new(413, "Image too large for the update slot"));
    }
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&bytes[24..56]);
    Ok(Header {
        version,
        length,
        digest,
    })
}

/// The public key from 64 hexadecimal characters (either case): `None` when it is not 64 hex
/// characters, not a point of the curve, or a weak (small-order) key.
pub fn parse_public_key(text: &str) -> Option<VerifyingKey> {
    let text = text.as_bytes();
    if text.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let high = char::from(text[2 * i]).to_digit(16)?;
        let low = char::from(text[2 * i + 1]).to_digit(16)?;
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    let key = VerifyingKey::from_bytes(&bytes).ok()?;
    (!key.is_weak()).then_some(key)
}

/// The inactive slot and the boot record, behind the platform (files at the host, ESP-IDF's OTA
/// on the box). Errors are a message. The boot record changes in `finish`, `confirm` and
/// `roll_back` only, each in one step that a power cut cannot leave half done.
pub trait UpdateAdapter {
    /// The inactive slot's size in bytes.
    fn capacity(&self) -> u64;
    /// Opens the inactive slot for an image of `length` bytes.
    fn begin(&mut self, length: u32) -> Result<(), String>;
    /// Writes the next bytes, in order.
    fn write(&mut self, chunk: &[u8]) -> Result<(), String>;
    /// Drops what was written: the inactive slot is not bootable, the boot record unchanged.
    fn abort(&mut self);
    /// Closes the slot, has the platform check its image, and makes it the next boot, on
    /// probation.
    fn finish(&mut self) -> Result<(), String>;
    /// True when the running image is on probation (booted from an update, not confirmed yet).
    fn pending(&self) -> bool;
    /// The running image is kept from now on.
    fn confirm(&mut self) -> Result<(), String>;
    /// The running image is marked bad and the previous one becomes the next boot.
    fn roll_back(&mut self) -> Result<(), String>;
}
