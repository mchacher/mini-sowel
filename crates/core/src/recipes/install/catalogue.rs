//! The catalogue of recipes, `recipes.json` (spec 042 R2): unsigned, each file it points to being
//! signed and pinned by its SHA-256. Read whole into RAM (at most 32 KB), refused as a whole when
//! over a bound, not JSON, of another format, or when an entry's address breaks the URL rule.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

/// The texts of one language of an entry (`i18n.<lang>`): what the store's page shows, nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LangPack {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<BTreeMap<String, SlotText>>,
}

/// The name and description of one slot in one language.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SlotText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One recipe of the catalogue, as `recipes.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i18n: Option<BTreeMap<String, LangPack>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The signed file's size, in bytes: at most 32 KB and a header.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(n: usize) -> serde_json::Value {
        json!({
            "id": format!("r{n}"), "version": "1.0.0", "name": "N", "description": "D",
            "size": 100, "sha256": "ab".repeat(32),
            "url": format!("https://example.com/r{n}.solite"),
            "i18n": {"fr": {"name": "Nom", "description": "Desc", "extra": 1,
                "slots": {"zone": {"name": "Zone", "description": "La zone", "x": true}}}},
            "unknownField": {"a": [1, 2]}
        })
    }

    fn body(entries: Vec<serde_json::Value>) -> Vec<u8> {
        serde_json::to_vec(&json!({"format": 1, "recipes": entries})).unwrap()
    }

    #[test]
    fn a_catalogue_of_32_entries_is_read_typed_with_its_i18n() {
        let entries = parse(&body((0..32).map(entry).collect())).unwrap();
        assert_eq!(entries.len(), 32);
        let fr = &entries[3].i18n.as_ref().unwrap()["fr"];
        assert_eq!(fr.name.as_deref(), Some("Nom"));
        let slot = &fr.slots.as_ref().unwrap()["zone"];
        assert_eq!(slot.description.as_deref(), Some("La zone"));
    }

    #[test]
    fn unknown_fields_are_ignored_and_not_served() {
        let entries = parse(&body(vec![entry(1)])).unwrap();
        let served = serde_json::to_value(&entries[0]).unwrap();
        assert!(served.get("unknownField").is_none());
        assert!(served["i18n"]["fr"].get("extra").is_none());
        assert_eq!(served["i18n"]["fr"]["slots"]["zone"]["name"], "Zone");
    }
}
