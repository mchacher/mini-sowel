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
/// link started`: for the images without a link (the box's, QEMU's and the measuring one, which
/// detect no cable: `cable` is what they pass). The radio image uses `boot_lines_radio`.
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

/// The radio image's `network:` lines (spec 022): as `boot_lines`, but the Wi-Fi link is started
/// by the image, so its line has no `no link started`. `ethernet` is not in this image.
pub fn boot_lines_radio(text: Option<&str>, cable: bool) -> Vec<String> {
    let (setting, fallback) = parse(text);
    let mut lines = Vec::new();
    if let Some(reason) = fallback {
        lines.push(reason.message());
    }
    lines.push(match choose(setting, cable) {
        Link::Wifi => format!("network: wifi (setting {})", setting.as_str()),
        Link::Ethernet => format!(
            "network: ethernet (setting {}), not in this image, no link started",
            setting.as_str()
        ),
    });
    lines
}

// ---------------------------------------------------------------------------------------------
// The Wi-Fi settings (spec 022 R1): `/data/wifi.json`
// ---------------------------------------------------------------------------------------------

/// The longest Wi-Fi settings text read; a longer one is refused, not parsed.
pub const MAX_WIFI_BYTES: usize = 256;
/// The longest SSID, in bytes (802.11).
const MAX_SSID_BYTES: usize = 32;

/// The Wi-Fi network to join. Neither the name nor the password is ever shown: not in `Debug`, not
/// in a log line (the name is personal data, the logs may be pasted into the repository).
#[derive(Clone, PartialEq, Eq)]
pub struct WifiSettings {
    ssid: String,
    password: String,
}

impl WifiSettings {
    /// The network's name, for the radio only.
    pub fn ssid(&self) -> &str {
        &self.ssid
    }

    /// The passphrase (empty for an open network), for the radio only.
    pub fn password(&self) -> &str {
        &self.password
    }

    /// The text the console writes to `/data/wifi.json` (spec 023 R6.2): `{"ssid":..,
    /// "password":..}`, which `wifi_settings` reads back to the same settings. Never logged.
    pub fn to_json(&self) -> String {
        #[derive(serde::Serialize)]
        struct Written<'a> {
            ssid: &'a str,
            password: &'a str,
        }
        serde_json::to_string(&Written {
            ssid: &self.ssid,
            password: &self.password,
        })
        // Two strings never fail to serialise.
        .unwrap_or_default()
    }

    /// What can be said about the password: `open network` or `password: <n> characters`.
    pub fn password_summary(&self) -> String {
        if self.password.is_empty() {
            "open network".to_string()
        } else {
            format!("password: {} characters", self.password.chars().count())
        }
    }
}

impl std::fmt::Debug for WifiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WifiSettings {{ {} }}", self.password_summary())
    }
}

/// Why a Wi-Fi settings text is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiRefusal {
    /// An empty or blank text.
    Empty,
    /// More than `MAX_WIFI_BYTES`.
    TooLong,
    /// Not a JSON object of `ssid` and `password` strings only.
    Invalid,
    InvalidSsid,
    InvalidPassword,
}

impl WifiRefusal {
    /// The reason, as the lines print it.
    pub fn message(self) -> &'static str {
        match self {
            WifiRefusal::Empty => "empty",
            WifiRefusal::TooLong => "too long",
            WifiRefusal::Invalid => "invalid",
            WifiRefusal::InvalidSsid => "invalid ssid",
            WifiRefusal::InvalidPassword => "invalid password",
        }
    }
}

fn password_valid(password: &str) -> bool {
    let printable = |c: char| (' '..='~').contains(&c);
    match password.len() {
        0 => true,
        8..=63 => password.chars().all(printable),
        64 => password.chars().all(|c| c.is_ascii_hexdigit()),
        _ => false,
    }
}

/// Reads `{"ssid": "<name>", "password": "<passphrase>"}`: at most `MAX_WIFI_BYTES`, an SSID of 1
/// to 32 bytes, a password empty, of 8 to 63 printable ASCII characters, or 64 hexadecimal digits,
/// and no other field.
pub fn wifi_settings(text: &str) -> Result<WifiSettings, WifiRefusal> {
    if text.len() > MAX_WIFI_BYTES {
        return Err(WifiRefusal::TooLong);
    }
    if text.trim().is_empty() {
        return Err(WifiRefusal::Empty);
    }
    let Ok(Value::Object(mut object)) = serde_json::from_str::<Value>(text) else {
        return Err(WifiRefusal::Invalid);
    };
    let (Some(Value::String(ssid)), Some(Value::String(password))) =
        (object.remove("ssid"), object.remove("password"))
    else {
        return Err(WifiRefusal::Invalid);
    };
    if !object.is_empty() {
        return Err(WifiRefusal::Invalid);
    }
    if ssid.is_empty() || ssid.len() > MAX_SSID_BYTES {
        return Err(WifiRefusal::InvalidSsid);
    }
    if !password_valid(&password) {
        return Err(WifiRefusal::InvalidPassword);
    }
    Ok(WifiSettings { ssid, password })
}

// ---------------------------------------------------------------------------------------------
// The link's lines and retries (spec 022 R2)
// ---------------------------------------------------------------------------------------------

/// The longest wait between two connection attempts, in milliseconds.
pub const WIFI_RETRY_MAX_MS: u64 = 60_000;
/// The first wait, in milliseconds.
const WIFI_RETRY_FIRST_MS: u64 = 1_000;

/// The waits between connection attempts: 1, 2, 4... seconds, then 60 until `reset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    next_ms: u64,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff::new()
    }
}

impl Backoff {
    pub fn new() -> Backoff {
        Backoff {
            next_ms: WIFI_RETRY_FIRST_MS,
        }
    }

    /// The wait before the next attempt, in milliseconds; the one after is doubled, up to
    /// `WIFI_RETRY_MAX_MS`.
    pub fn next_delay_ms(&mut self) -> u64 {
        let delay = self.next_ms;
        self.next_ms = (self.next_ms * 2).min(WIFI_RETRY_MAX_MS);
        delay
    }

    /// Back to the first wait (after a connection).
    pub fn reset(&mut self) {
        self.next_ms = WIFI_RETRY_FIRST_MS;
    }
}

/// ESP-IDF's disconnection code, in words.
pub fn wifi_reason(code: u16) -> String {
    match code {
        201 => "network not found".to_string(),
        15 | 202 | 204 | 2 => "wrong password or refused".to_string(),
        200 => "beacon lost".to_string(),
        other => format!("reason {other}"),
    }
}

/// The line when `/data/wifi.json` does not exist.
pub fn no_settings_line() -> String {
    "wifi: no settings (/data/wifi.json missing), no link started: sh scripts/box.sh wifi"
        .to_string()
}

/// The line when the settings are refused.
pub fn refused_line(refusal: WifiRefusal) -> String {
    format!(
        "wifi: settings refused ({}), no link started",
        refusal.message()
    )
}

/// The line when the settings are read: only what can be said about the password.
pub fn loaded_line(settings: &WifiSettings) -> String {
    format!("wifi: settings read, {}", settings.password_summary())
}

/// The line after a failed attempt: `not connected` before the first connection, `lost` after one.
pub fn retry_line(reason: &str, connected_before: bool, delay_ms: u64) -> String {
    let state = if connected_before {
        "lost"
    } else {
        "not connected"
    };
    format!(
        "wifi: {state} ({reason}), retry in {} s",
        delay_ms.div_ceil(1000)
    )
}

/// The line when the link is up.
pub fn connected_line(
    address: std::net::Ipv4Addr,
    name: &str,
    channel: u8,
    rssi_dbm: i8,
) -> String {
    format!(
        "wifi: connected, address {address}, name {name}, channel {channel}, rssi {rssi_dbm} dBm"
    )
}

// ---------------------------------------------------------------------------------------------
// The serial console's lines (spec 022 R3)
// ---------------------------------------------------------------------------------------------

/// The longest console line, in bytes, without its `\r` and `\n`; a longer one is never parsed.
pub const MAX_CONSOLE_LINE: usize = 512;

/// What one console line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleCommand {
    /// `wifi-set <JSON>`, accepted.
    WifiSet(WifiSettings),
    /// `wifi-set <JSON>`, refused for this reason.
    Refused(WifiRefusal),
    /// `wifi-forget`.
    WifiForget,
    /// `pair`: open the pairing window for `PAIR_AT_BOOT_SECONDS`, whatever the boot was.
    Pair,
    Empty,
    Unknown,
    /// More than `MAX_CONSOLE_LINE` bytes.
    TooLong,
}

/// Reads one console line (as many bytes as the reader kept, `\n` and one `\r` before it
/// dropped). The `Debug` form of the result holds neither the password nor the SSID.
pub fn parse_console_line(line: &[u8]) -> ConsoleCommand {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.len() > MAX_CONSOLE_LINE {
        return ConsoleCommand::TooLong;
    }
    let Ok(text) = std::str::from_utf8(line) else {
        return ConsoleCommand::Unknown;
    };
    if text.is_empty() {
        return ConsoleCommand::Empty;
    }
    let (command, rest) = match text.split_once(' ') {
        Some((command, rest)) => (command, Some(rest)),
        None => (text, None),
    };
    match (command, rest) {
        ("wifi-set", rest) => match wifi_settings(rest.unwrap_or("")) {
            Ok(settings) => ConsoleCommand::WifiSet(settings),
            Err(refusal) => ConsoleCommand::Refused(refusal),
        },
        ("wifi-forget", None) => ConsoleCommand::WifiForget,
        ("pair", None) => ConsoleCommand::Pair,
        _ => ConsoleCommand::Unknown,
    }
}
