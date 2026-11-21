//! The box's network link (spec 015 R5): one link, Wi-Fi or Ethernet, chosen once at boot from
//! the setting and the cable. The core makes the choice; the firmware starts the link.

use serde_json::Value;

/// The longest setting text read; a longer one is not parsed. Bounds the memory of the read.
pub const MAX_SETTING_BYTES: usize = 256;

/// The setting `{"link": "auto" | "wifi" | "ethernet"}`; values are case-sensitive, as Sowel's
/// enums are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkSetting {
    /// Ethernet when a cable is detected at boot, Wi-Fi otherwise. The default.
    Auto,
    Wifi,
    Ethernet,
}

/// The link started at boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Wifi,
    Ethernet,
}

/// Why a setting fell back to `auto`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fallback {
    /// No text, or an object without `link`.
    Missing,
    /// An empty or blank text.
    Empty,
    /// More than `MAX_SETTING_BYTES`.
    TooLong,
    /// Not JSON, not an object, or `link` not a string.
    Invalid,
    /// A string other than `auto`, `wifi`, `ethernet`.
    Unknown(String),
}

impl Fallback {
    /// The one-line text for the log.
    pub fn message(&self) -> String {
        match self {
            Fallback::Missing => "network: no setting, auto".into(),
            Fallback::Empty => "network: setting empty, auto".into(),
            Fallback::TooLong => {
                format!("network: setting over {MAX_SETTING_BYTES} bytes, auto")
            }
            Fallback::Invalid => "network: setting invalid, auto".into(),
            Fallback::Unknown(value) => format!("network: setting unknown {value:?}, auto"),
        }
    }
}

/// Reads the setting from its JSON text. Anything but a valid value gives `Auto` and the reason.
pub fn parse(text: Option<&str>) -> (NetworkSetting, Option<Fallback>) {
    let fallback = |reason| (NetworkSetting::Auto, Some(reason));
    let Some(text) = text else {
        return fallback(Fallback::Missing);
    };
    if text.len() > MAX_SETTING_BYTES {
        return fallback(Fallback::TooLong);
    }
    if text.trim().is_empty() {
        return fallback(Fallback::Empty);
    }
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(text) else {
        return fallback(Fallback::Invalid);
    };
    match object.get("link") {
        None => fallback(Fallback::Missing),
        Some(Value::String(value)) => match value.as_str() {
            "auto" => (NetworkSetting::Auto, None),
            "wifi" => (NetworkSetting::Wifi, None),
            "ethernet" => (NetworkSetting::Ethernet, None),
            other => fallback(Fallback::Unknown(other.into())),
        },
        Some(_) => fallback(Fallback::Invalid),
    }
}

/// The link to start: `wifi` and `ethernet` as asked, `auto` follows the cable. Chosen once;
/// nothing switches while running.
pub fn choose(setting: NetworkSetting, cable: bool) -> Link {
    match setting {
        NetworkSetting::Wifi => Link::Wifi,
        NetworkSetting::Ethernet => Link::Ethernet,
        NetworkSetting::Auto if cable => Link::Ethernet,
        NetworkSetting::Auto => Link::Wifi,
    }
}

impl NetworkSetting {
    /// The setting's value as it is written in the file.
    pub fn as_str(self) -> &'static str {
        match self {
            NetworkSetting::Auto => "auto",
            NetworkSetting::Wifi => "wifi",
            NetworkSetting::Ethernet => "ethernet",
        }
    }
}

impl Link {
    pub fn as_str(self) -> &'static str {
        match self {
            Link::Wifi => "wifi",
            Link::Ethernet => "ethernet",
        }
    }
}

/// The lines the firmware logs at boot for the setting's text: the fallback reason when the
/// setting fell back to `auto`, then the choice, for example `network: wifi (setting auto), no
/// link started` (the firmware starts no link and detects no cable: `cable` is what it passes).
pub fn boot_lines(text: Option<&str>, cable: bool) -> Vec<String> {
    let (setting, fallback) = parse(text);
    let mut lines = Vec::new();
    if let Some(reason) = fallback {
        lines.push(reason.message());
    }
    lines.push(format!(
        "network: {} (setting {}), no link started",
        choose(setting, cable).as_str(),
        setting.as_str()
    ));
    lines
}
