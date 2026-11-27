//! The home's settings (spec 026 R1): Sowel's flat key-value map of strings (`src/api/routes/
//! settings.ts`, the keys of `src/zones/sunlight-manager.ts`), limited to the nine keys Solite knows,
//! each checked by its rule. No I/O: the home keeps them, the saver writes `settings.json`.

pub mod sun;
pub mod tz;

use serde_json::{Map, Value};

use crate::failure::{Failure, Outcome};
use tz::TimeZone;

/// The keys Solite knows, in the order they are written; any other is refused. Bound: nine values.
pub const KEYS: [&str; 9] = [
    NAME,
    LATITUDE,
    LONGITUDE,
    TIMEZONE,
    SUNRISE_OFFSET,
    SUNSET_OFFSET,
    MANIFEST_URL,
    AUTO_CHECK,
    RECIPES_CATALOGUE,
];
pub const NAME: &str = "home.name";
pub const LATITUDE: &str = "home.latitude";
pub const LONGITUDE: &str = "home.longitude";
/// Solite's own key (decision D41).
pub const TIMEZONE: &str = "home.timezone";
pub const SUNRISE_OFFSET: &str = "home.sunriseOffset";
pub const SUNSET_OFFSET: &str = "home.sunsetOffset";
/// Solite's own keys (spec 039, D67): the manifest's address and the automatic check.
pub const MANIFEST_URL: &str = "update.manifestUrl";
pub const AUTO_CHECK: &str = "update.autoCheck";
/// Solite's own key (spec 042, D67's pattern): the recipe catalogue's address.
pub const RECIPES_CATALOGUE: &str = "recipes.catalogue";

/// The longest `update.manifestUrl`, in characters.
pub const MAX_MANIFEST_URL_CHARS: usize = 512;

/// The longest home name, in characters, after trimming. The numbers are bounded by their pattern
/// (17 characters at most); a time zone is a name of the list.
pub const MAX_HOME_NAME_CHARS: usize = 64;

/// Sowel's defaults of the offsets, in minutes.
const DEFAULT_SUNRISE_OFFSET: i64 = 30;
const DEFAULT_SUNSET_OFFSET: i64 = 45;

/// The nine values, `None` when unset; each set value passed its rule.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    values: [Option<String>; 9],
}

/// `^-?[0-9]{1,int}(\.[0-9]{1,frac})?$`, ASCII digits only; the value, or `None`.
fn decimal(text: &str, int: usize, frac: usize) -> Option<f64> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (digits, None),
    };
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(whole) || whole.len() > int {
        return None;
    }
    if let Some(fraction) = fraction
        && (!all_digits(fraction) || fraction.len() > frac)
    {
        return None;
    }
    text.parse::<f64>().ok()
}

/// The value of a key checked by its rule, as given (Sowel stores it untouched); `None` when it
/// breaks the rule. The empty string is not checked here: it is stored, and reads as unset, as in
/// Sowel (`!lat`).
fn check(key: &str, value: &str) -> Option<String> {
    let ok = match key {
        NAME => value.chars().count() <= MAX_HOME_NAME_CHARS,
        LATITUDE => decimal(value, 3, 12).is_some_and(|v| (-90.0..=90.0).contains(&v)),
        LONGITUDE => decimal(value, 3, 12).is_some_and(|v| (-180.0..=180.0).contains(&v)),
        TIMEZONE => tz::is_known(value),
        SUNRISE_OFFSET | SUNSET_OFFSET => {
            decimal(value, 3, 0).is_some_and(|v| (-180.0..=180.0).contains(&v))
        }
        MANIFEST_URL => {
            value.chars().count() <= MAX_MANIFEST_URL_CHARS
                && crate::update::release::url_allowed(value)
        }
        AUTO_CHECK => value == "true" || value == "false",
        RECIPES_CATALOGUE => crate::recipes::install::catalogue_url_allowed(value),
        _ => false,
    };
    ok.then(|| value.to_string())
}

fn index_of(key: &str) -> Option<usize> {
    KEYS.iter().position(|k| *k == key)
}

/// One checked entry: the key's index and its new value (`None`: unset).
pub type Entry = (usize, Option<String>);

impl Settings {
    /// The value of a key, as stored.
    pub fn get(&self, key: &str) -> Option<&str> {
        index_of(key).and_then(|i| self.values[i].as_deref())
    }

    /// The keys that are set and their values, in the order of `KEYS`.
    pub fn to_map(&self) -> Map<String, Value> {
        KEYS.iter()
            .zip(&self.values)
            .filter_map(|(key, value)| {
                value
                    .as_ref()
                    .map(|v| ((*key).to_string(), Value::String(v.clone())))
            })
            .collect()
    }

    /// How many keys are stored (an empty value included, as Sowel keeps it).
    pub fn count(&self) -> usize {
        self.values.iter().filter(|v| v.is_some()).count()
    }

    /// The JSON object of `settings.json` and `GET /settings` (`{}` when nothing is set).
    pub fn to_json(&self) -> String {
        Value::Object(self.to_map()).to_string()
    }

    /// Checks every entry of a body before anything changes (R1.3): an unknown key is 400
    /// `Unknown setting: <key>`, a value that is not a string or breaks its rule 400 `Invalid value
    /// for <key>`. The values are kept as given; the empty string is stored and reads as unset.
    pub fn check(entries: &Map<String, Value>) -> Outcome<Vec<Entry>> {
        let mut checked = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            let Some(index) = index_of(key) else {
                return Err(Failure::new(400, format!("Unknown setting: {key}")));
            };
            let invalid = || Failure::new(400, format!("Invalid value for {key}"));
            let Value::String(text) = value else {
                return Err(invalid());
            };
            if text.is_empty() {
                checked.push((index, Some(String::new())));
            } else {
                checked.push((index, Some(check(key, text).ok_or_else(invalid)?)));
            }
        }
        Ok(checked)
    }

    /// Applies checked entries; the keys whose value changed.
    pub fn apply(&mut self, entries: Vec<Entry>) -> Vec<&'static str> {
        let mut changed = Vec::new();
        for (index, value) in entries {
            if self.values[index] != value {
                self.values[index] = value;
                changed.push(KEYS[index]);
            }
        }
        changed
    }

    /// Settings made of checked entries alone (the others unset).
    pub fn from_entries(entries: Vec<Entry>) -> Settings {
        let mut settings = Settings::default();
        settings.apply(entries);
        settings
    }

    /// The keys whose value differs between the two, in the order of `KEYS`.
    pub fn differences(&self, other: &Settings) -> Vec<&'static str> {
        KEYS.iter()
            .zip(self.values.iter().zip(&other.values))
            .filter(|(_, (a, b))| a != b)
            .map(|(key, _)| *key)
            .collect()
    }

    /// The text of `settings.json`, read leniently (R1.4): a text that is not a JSON object of
    /// strings is the error; an unknown key or a value that breaks its rule is dropped with a
    /// warning. The settings, and true when something was dropped (the file is to be written
    /// again).
    pub fn parse_file(text: &str) -> Result<(Settings, bool), String> {
        let map: Map<String, Value> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut settings = Settings::default();
        let mut dropped = false;
        for (key, value) in &map {
            let Value::String(text) = value else {
                return Err(format!("the value of {key} is not a string"));
            };
            let Some(index) = index_of(key) else {
                log::warn!("settings: unknown setting {key} dropped");
                dropped = true;
                continue;
            };
            if text.is_empty() {
                settings.values[index] = Some(String::new());
                continue;
            }
            match check(key, text) {
                Some(value) => settings.values[index] = Some(value),
                None => {
                    log::warn!("settings: invalid value for {key} dropped");
                    dropped = true;
                }
            }
        }
        Ok((settings, dropped))
    }

    /// Latitude and longitude, when both are set.
    pub fn location(&self) -> Option<(f64, f64)> {
        let latitude = self.get(LATITUDE)?.parse::<f64>().ok()?;
        let longitude = self.get(LONGITUDE)?.parse::<f64>().ok()?;
        Some((latitude, longitude))
    }

    /// The home's time zone; UTC when unset.
    pub fn time_zone(&self) -> TimeZone {
        self.get(TIMEZONE)
            .and_then(tz::find)
            .unwrap_or(TimeZone::UTC)
    }

    fn offset(&self, key: &str, default: i64) -> i64 {
        self.get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(default)
    }

    /// Minutes after sunrise the day starts (30 when unset).
    pub fn sunrise_offset(&self) -> i64 {
        self.offset(SUNRISE_OFFSET, DEFAULT_SUNRISE_OFFSET)
    }

    /// Minutes before sunset the day ends (45 when unset).
    pub fn sunset_offset(&self) -> i64 {
        self.offset(SUNSET_OFFSET, DEFAULT_SUNSET_OFFSET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_number_patterns_take_plain_decimals_only() {
        for ok in ["48.8566", "-0", "0", "90", "-90", "1.123456789012"] {
            assert!(check(LATITUDE, ok).is_some(), "{ok}");
        }
        for bad in [
            "1e1",
            "+5",
            ".5",
            "5.",
            "030.",
            "0x10",
            "٤٥",
            "1234",
            "NaN",
            "inf",
            "-",
            "",
            "1.1234567890123",
            "4 5",
        ] {
            assert!(check(LATITUDE, bad).is_none(), "{bad}");
        }
        assert_eq!(check(SUNRISE_OFFSET, "030").as_deref(), Some("030"));
        assert!(check(SUNRISE_OFFSET, "30.0").is_none());
    }
}
