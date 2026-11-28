//! The signed script file, `.srs` (spec 042 R1): a header of 128 bytes, then the script's UTF-8
//! source. The header: magic `SOLITERS` (8 bytes); format `1` (u32, little-endian); the version
//! as three u16; 2 reserved bytes at zero; the source's length (u32); its SHA-256 (32 bytes); 8
//! reserved bytes at zero; the Ed25519 signature (64 bytes) over bytes 0 to 63. The recipe key
//! signs it, never the firmware's: the two are two keys with two magics.

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::update::{HEADER_BYTES, Version};

/// The whole file is at most this long (the API's body limit), so a source is at most 32 KB
/// (the body bound counts the header).
pub const MAX_FILE_BYTES: usize = crate::api::MAX_BODY_BYTES;
/// The longest source a file holds.
pub const MAX_SOURCE_BYTES: usize = MAX_FILE_BYTES - HEADER_BYTES;
/// The magic of a signed script file (the firmware's is `SOLITEUP`).
pub const MAGIC: &[u8; 8] = b"SOLITERS";
/// The format this program reads.
pub const FORMAT: u32 = 1;
/// The bytes the signature covers: the header without the signature.
const SIGNED_LEN: usize = 64;

/// A file that passed R1.2's checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub version: Version,
    pub source: String,
}

fn le16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

fn le32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// The 64 bytes the signature covers, for `--recipe-sign` and the tests.
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

/// SHA-256 of a source, as the header carries it.
pub fn digest(source: &[u8]) -> [u8; 32] {
    Sha256::digest(source).into()
}

/// Checks a file, in the order of R1.2: its length, the magic, the format, the reserved bytes, the
/// length field, the SHA-256, the signature (`verify_strict`). The error is the reason, which the
/// API answers with 400.
pub fn check(bytes: &[u8], key: &VerifyingKey) -> Result<Signed, String> {
    if bytes.len() <= HEADER_BYTES {
        return Err("Recipe file too short".to_string());
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!(
            "Recipe file too large (at most {MAX_FILE_BYTES} bytes)"
        ));
    }
    if &bytes[..8] != MAGIC {
        return Err("Not a Solite recipe file".to_string());
    }
    let format = le32(&bytes[8..12]);
    if format != FORMAT {
        return Err(format!("format {format} is not supported"));
    }
    if le16(&bytes[18..20]) != 0 || bytes[56..64].iter().any(|b| *b != 0) {
        return Err("Malformed recipe header".to_string());
    }
    let source = &bytes[HEADER_BYTES..];
    if le32(&bytes[20..24]) as usize != source.len() {
        return Err("Recipe length differs from the header".to_string());
    }
    if digest(source) != bytes[24..56] {
        return Err("Recipe SHA-256 differs from the header".to_string());
    }
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&bytes[64..HEADER_BYTES]);
    if key
        .verify_strict(&bytes[..SIGNED_LEN], &Signature::from_bytes(&signature))
        .is_err()
    {
        return Err("Invalid signature".to_string());
    }
    let source = std::str::from_utf8(source)
        .map_err(|_| "Recipe source is not UTF-8".to_string())?
        .to_string();
    Ok(Signed {
        version: Version::new(
            le16(&bytes[12..14]),
            le16(&bytes[14..16]),
            le16(&bytes[16..18]),
        ),
        source,
    })
}
