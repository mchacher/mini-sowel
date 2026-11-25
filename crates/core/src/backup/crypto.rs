//! The keys and the cipher of the archive (R3, R8): PBKDF2 in slices, HMAC-SHA256 for the key
//! separation, the key check and the MAC, ChaCha20 for the secrets.

use chacha20::ChaCha20;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use super::PAUSE_EVERY_ITERATIONS;

pub(crate) type HmacSha256 = Hmac<Sha256>;

/// The text the key check is the HMAC of.
const KEY_CHECK_TEXT: &[u8] = b"solite backup key check";
const ENCRYPTION_LABEL: &[u8] = b"solite backup encryption";
const AUTHENTICATION_LABEL: &[u8] = b"solite backup authentication";

/// An HMAC-SHA256 under `key`. HMAC takes a key of any length, so the error branch is never
/// taken; it gives a zero key rather than a panic.
pub(crate) fn hmac(key: &[u8]) -> HmacSha256 {
    HmacSha256::new_from_slice(key).unwrap_or_else(|_| HmacSha256::new(&Default::default()))
}

pub(crate) fn hmac_once(key: &[u8], text: &[u8]) -> [u8; 32] {
    let mut mac = hmac(key);
    mac.update(text);
    mac.finalize().into_bytes().into()
}

/// PBKDF2-HMAC-SHA256 (RFC 8018) of one 32-byte block, the same result as
/// `auth::pbkdf2_sha256`, but calling `pause` after every `PAUSE_EVERY_ITERATIONS` iterations
/// (not after the last), so that the box's task watchdog is fed during a derivation of seconds.
pub(crate) fn derive_master(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    pause: &mut dyn FnMut(),
) -> [u8; 32] {
    let base = hmac(password);
    let mut mac = base.clone();
    mac.update(salt);
    mac.update(&1u32.to_be_bytes());
    let mut u: [u8; 32] = mac.finalize().into_bytes().into();
    let mut t = u;
    for completed in 2..=iterations {
        let mut mac = base.clone();
        mac.update(&u);
        u = mac.finalize().into_bytes().into();
        for (acc, byte) in t.iter_mut().zip(&u) {
            *acc ^= byte;
        }
        if completed % PAUSE_EVERY_ITERATIONS == 0 && completed < iterations {
            pause();
        }
    }
    t
}

/// The two keys the master gives, and the key check that proves the password.
pub(crate) struct Keys {
    pub encryption: [u8; 32],
    pub authentication: [u8; 32],
}

impl Keys {
    pub fn new(master: &[u8; 32]) -> Self {
        Keys {
            encryption: hmac_once(master, ENCRYPTION_LABEL),
            authentication: hmac_once(master, AUTHENTICATION_LABEL),
        }
    }

    /// The 32 bytes of the header that tell a wrong password from a right one.
    pub fn key_check(&self) -> [u8; 32] {
        hmac_once(&self.authentication, KEY_CHECK_TEXT)
    }

    /// The cipher of the entry at `index`: nonce = the header's 8 bytes then the index (big
    /// endian), block counter 0 (RFC 8439).
    pub fn cipher(&self, nonce: &[u8; 8], index: u32) -> ChaCha20 {
        let mut full = [0u8; 12];
        full[..8].copy_from_slice(nonce);
        full[8..].copy_from_slice(&index.to_be_bytes());
        ChaCha20::new(&self.encryption.into(), &full.into())
    }
}

/// Encrypts or decrypts `bytes` in place (a stream cipher: the same operation).
pub(crate) fn apply(cipher: &mut ChaCha20, bytes: &mut [u8]) {
    cipher.apply_keystream(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::pbkdf2_sha256;

    #[test]
    fn the_sliced_derivation_gives_the_same_bytes_as_the_accounts_pbkdf2() {
        for iterations in [1, 2, 4_999, 5_000, 5_001, 10_000, 12_345] {
            let mut pauses = 0;
            let sliced = derive_master(b"a password", b"a salt of 16 byt", iterations, &mut || {
                pauses += 1;
            });
            assert_eq!(
                sliced,
                pbkdf2_sha256(b"a password", b"a salt of 16 byt", iterations),
                "{iterations} iterations"
            );
            // Every 5,000 iterations, but not after the last.
            let expected = if iterations <= 5_000 {
                0
            } else {
                (iterations - 1) / 5_000
            };
            assert_eq!(pauses, expected, "{iterations} iterations");
        }
    }

    #[test]
    fn a_password_longer_than_the_hash_block_is_derived_like_the_accounts() {
        let long = vec![b'p'; 300];
        assert_eq!(
            derive_master(&long, b"salt", 3, &mut || {}),
            pbkdf2_sha256(&long, b"salt", 3)
        );
    }
}
