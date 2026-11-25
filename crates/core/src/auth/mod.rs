//! The home's account, its password and its sessions (spec 029, decisions D47 to D49). One account
//! (Sowel's admin), a PBKDF2-HMAC-SHA256 hash, opaque tokens kept in RAM, a rate limit on the
//! attempts, and the `Presence` adapter that says whether a person at the box confirmed. No I/O, no
//! clock and no random source of its own: the caller gives them.

mod file;
mod presence;
mod sessions;

use std::cell::{Cell, RefCell};

use pbkdf2::pbkdf2_hmac;
use serde::Serialize;
use sha2::Sha256;

use crate::devices::{Id, iso, iso_option};

pub use file::{parse_file, to_file_json};
pub use presence::{
    AlwaysConfirmed, BOOT_CONFIRM_MS, BootPresence, ButtonPresence, CONFIRM_WINDOW_MS,
    NeverConfirmed, Presence,
};
pub use sessions::{Attempts, Session, Sessions, token_hex};

/// PBKDF2 iterations of a new hash (D47): about 0.5 to 1 s on the ESP32's software SHA-256.
pub const PBKDF2_ITERATIONS: u32 = 10_000;
/// The fewest and the most iterations a stored hash may declare.
pub const MIN_ITERATIONS: u32 = 1_000;
pub const MAX_ITERATIONS: u32 = 1_000_000;
/// Lifetime of an access token, in seconds (Sowel's default).
pub const ACCESS_TTL_S: u64 = 900;
/// Lifetime of a refresh token, in seconds: 30 days (Sowel's default).
pub const REFRESH_TTL_S: u64 = 2_592_000;
/// Sessions kept in RAM; a ninth drops the one opened first.
pub const MAX_SESSIONS: usize = 8;
/// Attempts (setup and login together) accepted per `ATTEMPT_WINDOW_MS`.
pub const MAX_ATTEMPTS: usize = 10;
/// The window of the rate limit, in milliseconds.
pub const ATTEMPT_WINDOW_MS: u64 = 60_000;
/// The fewest and the most characters of a password, and its most bytes (UTF-8).
pub const MIN_PASSWORD_CHARS: usize = 8;
pub const MAX_PASSWORD_CHARS: usize = 128;
pub const MAX_PASSWORD_BYTES: usize = 512;
/// The most characters of a user name and of a display name.
pub const MAX_USERNAME_CHARS: usize = 64;
pub const MAX_DISPLAY_NAME_CHARS: usize = 64;
/// The scheme written in `auth.json`.
pub const SCHEME: &str = "pbkdf2-sha256";

/// Constant-time comparison: the time depends on the length only, never on where the bytes differ.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    text
}

/// `N` bytes from `2 * N` lowercase hexadecimal characters; `None` for anything else (upper case
/// included, so one token has one spelling).
pub(crate) fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let digits = text.as_bytes();
    if digits.len() != N * 2 {
        return None;
    }
    let value = |digit: u8| match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    };
    let mut out = [0u8; N];
    for (byte, pair) in out.iter_mut().zip(digits.chunks(2)) {
        *byte = (value(pair[0])? << 4) | value(pair[1])?;
    }
    Some(out)
}

// ---- What the tests observe of the hash ----

thread_local! {
    static HASHES: Cell<u32> = const { Cell::new(0) };
    static HOOK: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// How many password hashes this thread computed (a test counts them; it stops at its maximum).
pub fn hashes_on_this_thread() -> u32 {
    HASHES.with(Cell::get)
}

/// A function called on this thread each time a password hash starts, so that a test can look at
/// what is locked meanwhile (`None` removes it). Not part of the API of the tool.
#[doc(hidden)]
pub fn set_hash_hook(hook: Option<Box<dyn Fn()>>) {
    HOOK.with(|slot| *slot.borrow_mut() = hook);
}

/// PBKDF2-HMAC-SHA256 (RFC 8018) of a password: 32 bytes.
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    HASHES.with(|count| count.set(count.get().saturating_add(1)));
    HOOK.with(|slot| {
        if let Some(hook) = slot.borrow().as_ref() {
            hook();
        }
    });
    let mut out = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password, salt, iterations, &mut out);
    out
}

/// A password hash with what it takes to check it again: the iteration count is kept with it, so
/// a later change of `PBKDF2_ITERATIONS` still checks the old hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordHash {
    pub iterations: u32,
    pub salt: [u8; 16],
    pub hash: [u8; 32],
}

impl PasswordHash {
    /// The hash of `password` with the given salt and `PBKDF2_ITERATIONS`. Slow: never under the
    /// home's lock.
    pub fn new(password: &str, salt: [u8; 16]) -> Self {
        PasswordHash {
            iterations: PBKDF2_ITERATIONS,
            salt,
            hash: pbkdf2_sha256(password.as_bytes(), &salt, PBKDF2_ITERATIONS),
        }
    }

    /// True when `password` gives this hash (compared in constant time). Slow.
    pub fn verify(&self, password: &str) -> bool {
        let computed = pbkdf2_sha256(password.as_bytes(), &self.salt, self.iterations);
        ct_eq(&computed, &self.hash)
    }
}

/// The languages of Sowel's `UserPreferences`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Fr,
    En,
}

impl Language {
    pub fn parse(text: &str) -> Option<Language> {
        match text {
            "fr" => Some(Language::Fr),
            "en" => Some(Language::En),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Language::Fr => "fr",
            Language::En => "en",
        }
    }
}

/// True for a password of 8 to 128 characters and at most 512 bytes (R1.2).
pub fn password_ok(password: &str) -> bool {
    let chars = password.chars().count();
    (MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&chars)
        && password.len() <= MAX_PASSWORD_BYTES
}

/// True for 1 to 64 characters without a control character.
pub fn username_ok(name: &str) -> bool {
    let chars = name.chars().count();
    (1..=MAX_USERNAME_CHARS).contains(&chars) && !name.chars().any(char::is_control)
}

/// True for 1 to 64 characters.
pub fn display_name_ok(name: &str) -> bool {
    (1..=MAX_DISPLAY_NAME_CHARS).contains(&name.chars().count())
}

/// The home's one account. Its role is always `admin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: Id,
    pub username: String,
    pub display_name: String,
    pub language: Language,
    pub created_at: u64,
    pub password: PasswordHash,
}

/// Sowel's `UserPreferences`, `language` only.
#[derive(Debug, Serialize)]
pub struct Preferences {
    pub language: Language,
}

/// Sowel's `User` (`src/shared/types.ts`), as the API answers it: no password.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Id,
    pub username: String,
    pub display_name: String,
    pub role: &'static str,
    pub preferences: Preferences,
    pub enabled: bool,
    #[serde(with = "iso_option")]
    pub last_login_at: Option<u64>,
    #[serde(with = "iso")]
    pub created_at: u64,
}

impl Account {
    /// The account as Sowel's `User`; `last_login_at` is the last login since the start.
    pub fn user(&self, last_login_at: Option<u64>) -> User {
        User {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            role: "admin",
            preferences: Preferences {
                language: self.language,
            },
            enabled: true,
            last_login_at,
            created_at: self.created_at,
        }
    }
}

/// Sowel's `AuthTokens` (`src/auth/auth-service.ts`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub user: User,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_unhex_are_inverse_and_lower_case_only() {
        let bytes: [u8; 16] = [
            0, 1, 0xab, 0xff, 9, 10, 11, 12, 13, 14, 15, 16, 0xcd, 0xef, 0x10, 0x7f,
        ];
        let text = hex(&bytes);
        assert_eq!(text.len(), 32);
        assert_eq!(unhex::<16>(&text), Some(bytes));
        assert_eq!(unhex::<16>(&text.to_uppercase()), None);
        assert_eq!(unhex::<16>(&text[1..]), None);
        assert_eq!(unhex::<16>(&format!("{}g", &text[..31])), None);
    }

    #[test]
    fn the_constant_time_comparison_tells_equal_from_different() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        assert!(ct_eq(b"", b""));
    }

    #[test]
    fn the_password_and_name_rules_count_characters() {
        assert!(!password_ok("1234567"));
        assert!(password_ok("12345678"));
        assert!(password_ok(&"x".repeat(128)));
        assert!(!password_ok(&"x".repeat(129)));
        // 128 four-byte characters are 512 bytes: allowed; 129 are not.
        assert!(password_ok(&"😀".repeat(128)));
        assert!(!password_ok(&"😀".repeat(129)));
        assert!(username_ok("admin"));
        assert!(!username_ok(""));
        assert!(!username_ok("a\tb"));
        assert!(!username_ok(&"x".repeat(65)));
        assert!(display_name_ok(&"x".repeat(64)));
        assert!(!display_name_ok(""));
    }
}
