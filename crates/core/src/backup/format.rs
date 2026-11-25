//! The archive's bytes (R3), format version 1, all integers little-endian: the header, the
//! entries' framing and the manifest.

use serde::{Deserialize, Serialize};

pub(crate) const MAGIC: &[u8; 8] = b"SOLITEBK";
pub(crate) const FORMAT: u16 = 1;
/// Magic 8, format 2, iterations 4, salt 16, nonce 8, key check 32.
pub(crate) const HEADER_BYTES: usize = 70;
/// What `restore` reads first: the magic and the format.
pub(crate) const PREFIX_BYTES: usize = 10;
/// An entry: a file.
pub(crate) const KIND_FILE: u8 = 1;
/// The end: the manifest and the MAC.
pub(crate) const KIND_END: u8 = 0;
/// Bit 0 of an entry's flags: its bytes are encrypted.
pub(crate) const FLAG_ENCRYPTED: u8 = 1;
/// The MAC's length, in bytes.
pub(crate) const MAC_BYTES: usize = 32;

/// The header, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Header {
    pub iterations: u32,
    pub salt: [u8; 16],
    pub nonce: [u8; 8],
    pub key_check: [u8; 32],
}

impl Header {
    pub fn encode(&self) -> [u8; HEADER_BYTES] {
        let mut bytes = [0u8; HEADER_BYTES];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..10].copy_from_slice(&FORMAT.to_le_bytes());
        bytes[10..14].copy_from_slice(&self.iterations.to_le_bytes());
        bytes[14..30].copy_from_slice(&self.salt);
        bytes[30..38].copy_from_slice(&self.nonce);
        bytes[38..70].copy_from_slice(&self.key_check);
        bytes
    }

    /// The fields after the magic and the format (bytes 10 to 70).
    pub fn parse_rest(bytes: &[u8; HEADER_BYTES]) -> Header {
        let mut iterations = [0u8; 4];
        iterations.copy_from_slice(&bytes[10..14]);
        let mut header = Header {
            iterations: u32::from_le_bytes(iterations),
            salt: [0; 16],
            nonce: [0; 8],
            key_check: [0; 32],
        };
        header.salt.copy_from_slice(&bytes[14..30]);
        header.nonce.copy_from_slice(&bytes[30..38]);
        header.key_check.copy_from_slice(&bytes[38..70]);
        header
    }
}

/// One file of the manifest: the bytes stored (the ciphertext for an encrypted file), so that the
/// manifest says nothing about a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManifestFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub encrypted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub version: u16,
    pub solite: String,
    pub files: Vec<ManifestFile>,
}

impl Manifest {
    pub fn new(files: Vec<ManifestFile>) -> Self {
        Manifest {
            version: FORMAT,
            solite: crate::VERSION.to_string(),
            files,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        // A struct of strings and numbers: serialisation cannot fail; an empty manifest would be
        // refused by the restore.
        serde_json::to_vec(self).unwrap_or_default()
    }
}
