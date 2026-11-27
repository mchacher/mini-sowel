//! The catalogue of recipes, `recipes.json` (spec 042 R2): unsigned, each file it points to being
//! signed and pinned by its SHA-256. Read whole into RAM (at most 32 KB), refused as a whole when
//! over a bound, not JSON, of another format, or when an entry's address breaks the URL rule.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::file::MAX_FILE_BYTES;
use crate::update::Version;
use crate::update::release::url_allowed;

/// The catalogue is at most this long, in bytes.
pub const MAX_CATALOGUE_BYTES: usize = 32 * 1024;
/// The catalogue lists at most this many recipes.
pub const MAX_ENTRIES: usize = 32;
/// The longest address of a catalogue's entry, in characters (as the setting's).
pub const MAX_ENTRY_URL_CHARS: usize = 2048;
/// The catalogue's default address.
pub const DEFAULT_CATALOGUE_URL: &str =
    "https://github.com/mchacher/solite-recipes/releases/latest/download/recipes.json";

/// One recipe of the catalogue, as `recipes.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i18n: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The signed file's size, in bytes: at most 16 KB.
    pub size: u64,
    /// The signed file's SHA-256, 64 hexadecimal characters.
    pub sha256: String,
    pub url: String,
    /// The lowest firmware that runs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
}

impl Entry {
    /// The entry's version, checked when the catalogue is read.
    pub fn version(&self) -> Option<Version> {
        Version::parse(&self.version)
    }

    pub fn min_version(&self) -> Option<Version> {
        self.min_version.as_deref().and_then(Version::parse)
    }

    /// The signed file's SHA-256 as bytes.
    pub fn digest(&self) -> Option<[u8; 32]> {
        let text = self.sha256.as_bytes();
        if text.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            let high = char::from(text[2 * i]).to_digit(16)?;
            let low = char::from(text[2 * i + 1]).to_digit(16)?;
            *byte = u8::try_from(high * 16 + low).ok()?;
        }
        Some(out)
    }
}

#[derive(Debug, Deserialize)]
struct Document {
    format: Option<u64>,
    recipes: Option<Vec<Entry>>,
}

/// Reads a catalogue; the error is the reason shown as the store's `error`.
pub fn parse(body: &[u8]) -> Result<Vec<Entry>, String> {
    if body.len() > MAX_CATALOGUE_BYTES {
        return Err("Catalogue too large".to_string());
    }
    let document: Document =
        serde_json::from_slice(body).map_err(|e| format!("Invalid catalogue: {e}"))?;
    match document.format {
        Some(1) => {}
        Some(other) => return Err(format!("Unsupported catalogue format {other}")),
        None => return Err("Invalid catalogue: format".to_string()),
    }
    let Some(entries) = document.recipes else {
        return Err("Invalid catalogue: recipes".to_string());
    };
    if entries.len() > MAX_ENTRIES {
        return Err(format!(
            "Catalogue lists {} recipes, at most {MAX_ENTRIES}",
            entries.len()
        ));
    }
    for entry in &entries {
        let bad = |field: &str| format!("Invalid catalogue: {}: {field}", entry.id);
        if entry.version().is_none() {
            return Err(bad("version"));
        }
        if entry.min_version.is_some() && entry.min_version().is_none() {
            return Err(bad("minVersion"));
        }
        if entry.digest().is_none() {
            return Err(bad("sha256"));
        }
        if entry.size == 0 || entry.size > MAX_FILE_BYTES as u64 {
            return Err(bad("size"));
        }
        if entry.url.len() > MAX_ENTRY_URL_CHARS || !url_allowed(&entry.url) {
            return Err(bad("url"));
        }
    }
    Ok(entries)
}
