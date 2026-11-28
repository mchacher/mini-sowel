//! Scenario files: the simulated radio, its devices and what happens when, in plain JSON.
//!
//! Every error is one line, `scenario: <file>: ...`, naming the line and column for a JSON syntax
//! error, or the field (`devices[1].kind`, `events[0].at`) for anything else.

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::devices::{DeviceReport, Kind, button_command};
use crate::ncp::MAX_DEVICES;

/// The largest scenario file, in bytes (checked before it is read).
pub const MAX_FILE_BYTES: u64 = 1_048_576;
/// Most events a scenario holds.
pub const MAX_EVENTS: usize = 10_000;
/// The latest `at`, in seconds (24 h).
const MAX_AT_SECONDS: u64 = 24 * 3600;

/// Whether the radio starts with a network when it has no memory file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkInit {
    None,
    Formed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadioSpec {
    /// The EZSP protocol version (13 to 19).
    pub ezsp: u8,
    /// The stack version as the radio reports it (`7.4.4.0` is 0x7440).
    pub stack_version: u16,
    /// The radio's address, most significant byte first.
    pub eui64: [u8; 8],
    pub network: NetworkInit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSpec {
    pub id: String,
    pub kind: Kind,
    /// Most significant byte first.
    pub eui64: [u8; 8],
    pub manufacturer: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Join,
    Report(DeviceReport),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// Scenario time since the host connected, in milliseconds.
    pub at_ms: u64,
    /// Index in `Scenario::devices`.
    pub device: usize,
    pub action: Action,
}

/// A parsed, checked scenario: at most `MAX_DEVICES` devices and `MAX_EVENTS` events, the events
/// sorted by time (ties in file order).
#[derive(Clone, Debug, PartialEq)]
pub struct Scenario {
    pub radio: RadioSpec,
    pub devices: Vec<DeviceSpec>,
    pub events: Vec<Event>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScenario {
    radio: Option<Value>,
    devices: Option<Value>,
    events: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRadio {
    ezsp: Option<Value>,
    stack: Option<Value>,
    eui64: Option<Value>,
    network: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDevice {
    id: Option<Value>,
    kind: Option<Value>,
    eui64: Option<Value>,
    manufacturer: Option<Value>,
    model: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvent {
    at: Option<Value>,
    device: Option<Value>,
    #[serde(rename = "do")]
    action: Option<Value>,
    temperature: Option<Value>,
    humidity: Option<Value>,
    battery: Option<Value>,
    motion: Option<Value>,
    light_state: Option<Value>,
    color_temp: Option<Value>,
    color: Option<Value>,
    #[serde(rename = "action")]
    press: Option<Value>,
    position: Option<Value>,
    power: Option<Value>,
    current: Option<Value>,
    energy: Option<Value>,
}

/// Reads the raw structure of one object; `prefix` is the path of the object plus a dot
/// (`devices[1].`), empty for the top level.
fn raw<T: DeserializeOwned>(value: Value, prefix: &str) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| {
        let message = e.to_string();
        let field = message
            .split('`')
            .nth(1)
            .filter(|_| message.starts_with("unknown field"));
        match field {
            Some(field) => format!("{prefix}{field}: unknown field"),
            None if prefix.is_empty() => "the file must hold a JSON object".to_string(),
            None => format!("{}: must be an object", prefix.trim_end_matches('.')),
        }
    })
}

fn text_of(value: Option<Value>, path: &str) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("{path}: must be a text")),
    }
}

fn required_text(value: Option<Value>, path: &str) -> Result<String, String> {
    text_of(value, path)?.ok_or_else(|| format!("{path}: missing"))
}

fn eui64_of(text: &str, path: &str) -> Result<[u8; 8], String> {
    let bad = || format!("{path}: must be 16 hexadecimal digits, got \"{text}\"");
    if text.len() != 16 || !text.is_ascii() {
        return Err(bad());
    }
    let mut eui = [0u8; 8];
    for (i, byte) in eui.iter_mut().enumerate() {
        *byte = text
            .get(2 * i..2 * i + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .ok_or_else(bad)?;
    }
    Ok(eui)
}

/// `7.4.4.0` is 0x7440: four numbers from 0 to 15.
fn stack_of(text: &str, path: &str) -> Result<u16, String> {
    let bad = || format!("{path}: must be four numbers from 0 to 15 like 7.4.4.0, got \"{text}\"");
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return Err(bad());
    }
    let mut version = 0u16;
    for part in parts {
        let number: u16 = part.parse().map_err(|_| bad())?;
        if number > 15 {
            return Err(bad());
        }
        version = (version << 4) | number;
    }
    Ok(version)
}

/// `1m30s` is 90 seconds: whole numbers with `h`, `m`, `s` in that order, at most 24 h. The
/// result is in milliseconds.
pub fn parse_at(text: &str) -> Result<u64, String> {
    let invalid = || {
        format!(
            "invalid time \"{text}\" (hours, minutes and seconds in that order, like 1h30m or 90s)"
        )
    };
    if text.is_empty() {
        return Err(invalid());
    }
    let mut rest = text;
    let mut last_rank = 0;
    let mut seconds: u64 = 0;
    while !rest.is_empty() {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits > 6 {
            return Err(invalid());
        }
        let (number, after) = rest.split_at(digits);
        let number: u64 = number.parse().map_err(|_| invalid())?;
        let mut chars = after.chars();
        let (rank, unit_seconds) = match chars.next() {
            Some('h') => (1, 3600),
            Some('m') => (2, 60),
            Some('s') => (3, 1),
            _ => return Err(invalid()),
        };
        if rank <= last_rank {
            return Err(invalid());
        }
        last_rank = rank;
        seconds += number * unit_seconds;
        rest = chars.as_str();
    }
    if seconds > MAX_AT_SECONDS {
        return Err(format!("\"{text}\" is over 24 h"));
    }
    Ok(seconds * 1000)
}

fn radio_of(value: Option<Value>) -> Result<RadioSpec, String> {
    let mut radio = RadioSpec {
        ezsp: 13,
        stack_version: 0x7440,
        eui64: [0x8C, 0x6F, 0xB9, 0xFF, 0xFE, 0x12, 0x34, 0x56],
        network: NetworkInit::None,
    };
    let Some(value) = value else {
        return Ok(radio);
    };
    let spec: RawRadio = raw(value, "radio.")?;
    if let Some(value) = spec.ezsp {
        radio.ezsp = value
            .as_u64()
            .filter(|n| (13..=19).contains(n))
            .and_then(|n| u8::try_from(n).ok())
            .ok_or("radio.ezsp: must be a whole number from 13 to 19")?;
    }
    if let Some(text) = text_of(spec.stack, "radio.stack")? {
        radio.stack_version = stack_of(&text, "radio.stack")?;
    }
    if let Some(text) = text_of(spec.eui64, "radio.eui64")? {
        radio.eui64 = eui64_of(&text, "radio.eui64")?;
    }
    if let Some(text) = text_of(spec.network, "radio.network")? {
        radio.network = match text.as_str() {
            "none" => NetworkInit::None,
            "formed" => NetworkInit::Formed,
            _ => return Err("radio.network: must be \"none\" or \"formed\"".to_string()),
        };
    }
    Ok(radio)
}

/// A Zigbee Basic cluster string is at most 32 characters.
fn short_text(value: Option<Value>, path: &str) -> Result<Option<String>, String> {
    let text = text_of(value, path)?;
    match text {
        Some(text) if text.chars().count() > 32 => Err(format!("{path}: at most 32 characters")),
        other => Ok(other),
    }
}

/// A top-level list: absent is empty, anything but a list is an error naming the field.
fn list_of(value: Option<Value>, name: &str) -> Result<Vec<Value>, String> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(format!("{name}: must be a list")),
    }
}

fn devices_of(values: Vec<Value>) -> Result<Vec<DeviceSpec>, String> {
    let mut devices: Vec<DeviceSpec> = Vec::new();
    for (i, value) in values.into_iter().enumerate() {
        if i >= MAX_DEVICES {
            return Err(format!(
                "devices[{i}]: too many devices (at most {MAX_DEVICES})"
            ));
        }
        let prefix = format!("devices[{i}].");
        let spec: RawDevice = raw(value, &prefix)?;
        let id = required_text(spec.id, &format!("{prefix}id"))?;
        let kind_text = required_text(spec.kind, &format!("{prefix}kind"))?;
        let kind = Kind::from_name(&kind_text).ok_or_else(|| {
            format!(
                "{prefix}kind: unknown kind \"{kind_text}\" (temperature-humidity, motion, relay, dimmer, color-light, button, plug or shutter)"
            )
        })?;
        let eui_path = format!("{prefix}eui64");
        let eui64 = eui64_of(&required_text(spec.eui64, &eui_path)?, &eui_path)?;
        if devices.iter().any(|d| d.id == id) {
            return Err(format!("{prefix}id: duplicate id \"{id}\""));
        }
        if devices.iter().any(|d| d.eui64 == eui64) {
            return Err(format!("{eui_path}: duplicate EUI64"));
        }
        devices.push(DeviceSpec {
            id,
            kind,
            eui64,
            manufacturer: short_text(spec.manufacturer, &format!("{prefix}manufacturer"))?,
            model: short_text(spec.model, &format!("{prefix}model"))?,
        });
    }
    Ok(devices)
}

/// Which measures a kind of device can report.
fn allows(kind: Kind, field: &str) -> bool {
    match kind {
        Kind::TemperatureHumidity => matches!(field, "temperature" | "humidity" | "battery"),
        Kind::Motion => matches!(field, "motion" | "battery"),
        Kind::Relay | Kind::Dimmer => field == "light_state",
        Kind::Plug => matches!(field, "light_state" | "power" | "current" | "energy"),
        Kind::ColorLight => matches!(field, "light_state" | "color_temp" | "color"),
        Kind::Button => field == "action",
        Kind::Shutter => field == "position",
    }
}

fn number_in(value: &Value, low: f64, high: f64, path: &str) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|n| (low..=high).contains(n))
        .ok_or_else(|| format!("{path}: must be a number from {low} to {high}"))
}

/// The `color` of a report: an object with `x` and `y`, each 0 to 1, and nothing else.
fn color_of(value: &Value, path: &str) -> Result<(f64, f64), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{path}: must be an object with x and y"))?;
    if let Some(key) = object.keys().find(|k| *k != "x" && *k != "y") {
        return Err(format!("{path}.{key}: unknown field"));
    }
    let coordinate = |name: &str| {
        let field = format!("{path}.{name}");
        number_in(
            object
                .get(name)
                .ok_or_else(|| format!("{field}: required"))?,
            0.0,
            1.0,
            &field,
        )
    };
    Ok((coordinate("x")?, coordinate("y")?))
}

fn event_of(value: Value, i: usize, devices: &[DeviceSpec]) -> Result<Event, String> {
    let prefix = format!("events[{i}].");
    let spec: RawEvent = raw(value, &prefix)?;
    let at_text = required_text(spec.at, &format!("{prefix}at"))?;
    let at_ms = parse_at(&at_text).map_err(|reason| format!("{prefix}at: {reason}"))?;
    let device_text = required_text(spec.device, &format!("{prefix}device"))?;
    let device = devices
        .iter()
        .position(|d| d.id == device_text)
        .ok_or_else(|| format!("{prefix}device: unknown device \"{device_text}\""))?;
    let kind = devices[device].kind;
    let action_text = required_text(spec.action, &format!("{prefix}do"))?;
    let fields = [
        ("temperature", spec.temperature),
        ("humidity", spec.humidity),
        ("battery", spec.battery),
        ("motion", spec.motion),
        ("light_state", spec.light_state),
        ("color_temp", spec.color_temp),
        ("color", spec.color),
        ("action", spec.press),
        ("position", spec.position),
        ("power", spec.power),
        ("current", spec.current),
        ("energy", spec.energy),
    ];
    let action = match action_text.as_str() {
        "join" => {
            if let Some((name, _)) = fields.iter().find(|(_, v)| v.is_some()) {
                return Err(format!("{prefix}{name}: not allowed with \"join\""));
            }
            Action::Join
        }
        "report" => {
            let mut report = DeviceReport::default();
            let mut any = false;
            for (name, value) in fields {
                let Some(value) = value else { continue };
                let path = format!("{prefix}{name}");
                if !allows(kind, name) {
                    return Err(format!("{path}: not allowed for kind {}", kind.name()));
                }
                any = true;
                match name {
                    "temperature" => {
                        report.temperature = Some(number_in(&value, -50.0, 150.0, &path)?);
                    }
                    "humidity" => report.humidity = Some(number_in(&value, 0.0, 100.0, &path)?),
                    "battery" => {
                        report.battery = Some(
                            value
                                .as_u64()
                                .filter(|n| *n <= 100)
                                .and_then(|n| u8::try_from(n).ok())
                                .ok_or_else(|| {
                                    format!("{path}: must be a whole number from 0 to 100")
                                })?,
                        );
                    }
                    "motion" => {
                        report.motion = Some(
                            value
                                .as_bool()
                                .ok_or_else(|| format!("{path}: must be true or false"))?,
                        );
                    }
                    "color_temp" => {
                        // A whole number of mireds: 150 to 500 is inside a u16.
                        report.color_temp = Some(
                            value
                                .as_u64()
                                .filter(|n| (150..=500).contains(n))
                                .and_then(|n| u16::try_from(n).ok())
                                .ok_or_else(|| {
                                    format!("{path}: must be a whole number from 150 to 500")
                                })?,
                        );
                    }
                    "color" => report.color = Some(color_of(&value, &path)?),
                    "action" => {
                        report.action = Some(
                            value
                                .as_str()
                                .filter(|name| button_command(0, name).is_some())
                                .map(str::to_string)
                                .ok_or_else(|| {
                                    format!(
                                        "{path}: must be on, off, toggle, brightness_move_up or brightness_move_down"
                                    )
                                })?,
                        );
                    }
                    "position" => {
                        report.position = Some(
                            number_in(&value, 0.0, 100.0, &path)
                                .ok()
                                .filter(|n| n.fract() == 0.0)
                                .map(|n| n as u8)
                                .ok_or_else(|| {
                                    format!("{path}: must be a whole number from 0 to 100")
                                })?,
                        );
                    }
                    "power" => report.power = Some(number_in(&value, -3000.0, 3000.0, &path)?),
                    "current" => report.current = Some(number_in(&value, 0.0, 60.0, &path)?),
                    "energy" => report.energy = Some(number_in(&value, 0.0, 1_000_000.0, &path)?),
                    _ => {
                        report.light_state = Some(match value.as_str() {
                            Some("on") => true,
                            Some("off") => false,
                            _ => return Err(format!("{path}: must be \"on\" or \"off\"")),
                        });
                    }
                }
            }
            if !any {
                return Err(format!("{prefix}do: a report needs at least one measure"));
            }
            Action::Report(report)
        }
        other => {
            return Err(format!(
                "{prefix}do: unknown action \"{other}\" (join or report)"
            ));
        }
    };
    Ok(Event {
        at_ms,
        device,
        action,
    })
}

impl Scenario {
    /// Parses and checks the text of a scenario; `file` is the name shown in errors.
    pub fn parse(text: &str, file: &str) -> Result<Scenario, String> {
        Scenario::check(text).map_err(|reason| format!("scenario: {file}: {reason}"))
    }

    fn check(text: &str) -> Result<Scenario, String> {
        if text.len() as u64 > MAX_FILE_BYTES {
            return Err("file is over 1 MB".to_string());
        }
        let value: Value = serde_json::from_str(text).map_err(|e| {
            let message = e.to_string();
            let reason = message.split(" at line ").next().unwrap_or(&message);
            format!("line {}, column {}: {reason}", e.line(), e.column())
        })?;
        let top: RawScenario = raw(value, "")?;
        if top.radio.as_ref().is_some_and(|v| !v.is_object()) {
            return Err("radio: must be an object".to_string());
        }
        let radio = radio_of(top.radio)?;
        let devices = devices_of(list_of(top.devices, "devices")?)?;
        let values = list_of(top.events, "events")?;
        let mut events = Vec::new();
        for (i, value) in values.into_iter().enumerate() {
            if i >= MAX_EVENTS {
                return Err(format!(
                    "events[{i}]: too many events (at most {MAX_EVENTS})"
                ));
            }
            events.push(event_of(value, i, &devices)?);
        }
        // A stable sort: events at the same time stay in file order.
        events.sort_by_key(|event| event.at_ms);
        Ok(Scenario {
            radio,
            devices,
            events,
        })
    }

    /// Reads a scenario file. A file over 1 MB is refused before it is read.
    pub fn load(path: &Path) -> Result<Scenario, String> {
        let file = path.display().to_string();
        let fail = |reason: String| format!("scenario: {file}: {reason}");
        let io = |e: std::io::Error| {
            let text = e.to_string();
            fail(text.split(" (os error").next().unwrap_or(&text).to_string())
        };
        let size = fs::metadata(path).map_err(io)?.len();
        if size > MAX_FILE_BYTES {
            return Err(fail("file is over 1 MB".to_string()));
        }
        let text = fs::read_to_string(path).map_err(io)?;
        Scenario::parse(&text, &file)
    }
}
