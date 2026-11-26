//! The recipe SDK (spec 041): the contract between Solite's engine and a recipe. A recipe
//! implements `Recipe` and reads and commands the home through `Host` only; the engine gives it
//! events and clocks, and keeps the state text and listen set each call returns (no listen set: unchanged). The data that
//! crosses the boundary (events, views, results) is serde types with Sowel's names, and `bounds`
//! holds the bounds both sides share. No dependency on the core: a recipe is linked into the core
//! (`motion-light`) or is a script the core runs through the same trait.

pub mod bounds;
pub mod duration;
pub mod event;
pub mod host;
pub mod recipe;
pub mod time;
pub mod view;

use serde::{Deserialize, Serialize};

pub use event::{Event, EventValue, ListenEntry};
pub use host::{Host, OrderStatus};
pub use recipe::{
    Change, ChangedEquipment, Clock, Deadline, ErrorKind, LogLine, Pruned, Recipe, RecipeError,
    RunStats, Update,
};
pub use view::{DataBindingView, EquipmentView, OrderBindingView, Sunlight, ZoneData};

/// A log line's level (Sowel's `RecipeLogEntry.level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// An id in its compact form: the 16 bytes of `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`. A recipe
/// holding many ids keeps them this way (16 bytes each, against about 90 for a text).
pub type IdBytes = [u8; 16];

/// The bytes of an id written as the home writes it: 36 characters, hexadecimal digits (capitals
/// are read) and dashes after the 8th, 13th, 18th and 23rd. `None` for anything else.
pub fn parse_id(text: &str) -> Option<IdBytes> {
    let b = text.as_bytes();
    if b.len() != 36 || [8, 13, 18, 23].iter().any(|i| b[*i] != b'-') {
        return None;
    }
    let mut bytes = [0u8; 16];
    let mut digits = b.iter().filter(|c| **c != b'-');
    for byte in &mut bytes {
        let high = char::from(*digits.next()?).to_digit(16)?;
        let low = char::from(*digits.next()?).to_digit(16)?;
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    Some(bytes)
}

/// An id as the home writes it: lower case.
pub fn format_id(id: &IdBytes) -> String {
    let mut text = String::with_capacity(36);
    for (i, byte) in id.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            text.push('-');
        }
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// An id as the home writes it: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` in lower case. `None` for
/// anything that is not a 36-character id written with hexadecimal digits (capitals are read).
pub fn canonical_id(text: &str) -> Option<String> {
    parse_id(text).map(|id| format_id(&id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_read_in_capitals_and_written_in_lower_case() {
        let id = "0A1B2C3D-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
        assert_eq!(
            canonical_id(id).as_deref(),
            Some("0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d")
        );
        for bad in [
            "",
            "nope",
            "0a1b2c3d4e5f4a6b8c7d9e0f1a2b3c4d",
            "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4g",
            "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4-",
        ] {
            assert_eq!(canonical_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn an_id_round_trips_through_its_bytes() {
        let id = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
        let bytes = parse_id(id).unwrap();
        assert_eq!(bytes[0], 0x0a);
        assert_eq!(bytes[15], 0x4d);
        assert_eq!(format_id(&bytes), id);
    }
}
