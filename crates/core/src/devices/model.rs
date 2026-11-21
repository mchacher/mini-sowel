//! Sowel's device types (`src/shared/types.ts` of the Sowel repository: `Device` T:185,
//! `DeviceData` T:225, `DeviceOrder` T:236, `DeviceWithDetails` T:259), serialised with Sowel's
//! field names: camelCase, an absent optional field omitted, a nullable one `null`.

use std::borrow::Cow;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::time::{format_iso, parse_iso};

/// A UUID v4, kept as 16 bytes (a device and its data hold several): written and read as the
/// 36-character lowercase string Sowel uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Id([u8; 16]);

impl Id {
    /// A UUID v4 from 16 random bytes (the version and variant bits are set).
    pub fn from_random(mut bytes: [u8; 16]) -> Id {
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        Id(bytes)
    }

    /// Reads `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`; `None` for anything else.
    pub fn parse(text: &str) -> Option<Id> {
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
        Some(Id(bytes))
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Id, D::Error> {
        struct IdVisitor;
        impl Visitor<'_> for IdVisitor {
            type Value = Id;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a UUID")
            }
            fn visit_str<E: de::Error>(self, text: &str) -> Result<Id, E> {
                Id::parse(text).ok_or_else(|| E::custom("not a UUID"))
            }
        }
        deserializer.deserialize_str(IdVisitor)
    }
}

/// Dates are kept as milliseconds since 1970 (UTC) and written as ISO 8601 with milliseconds,
/// as Sowel does.
mod iso {
    use super::*;

    pub fn serialize<S: Serializer>(ms: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format_iso(*ms))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_iso(&text).ok_or_else(|| de::Error::custom("not an ISO 8601 date"))
    }
}

/// The same for a date that may be `null`.
mod iso_option {
    use super::*;

    pub fn serialize<S: Serializer>(ms: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        match ms {
            Some(ms) => serializer.serialize_str(&format_iso(*ms)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        match Option::<String>::deserialize(deserializer)? {
            None => Ok(None),
            Some(text) => parse_iso(&text)
                .map(Some)
                .ok_or_else(|| de::Error::custom("not an ISO 8601 date")),
        }
    }
}

/// The `integrationId` and the `source` of every device of mini-Sowel.
pub const INTEGRATION_ID: &str = "mini-sowel";

/// Sowel's `DeviceStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    Online,
    Offline,
    Unknown,
}

/// Sowel's `PowerSource` (`dc` is not used here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerSource {
    Mains,
    Battery,
    Unknown,
}

/// Sowel's `DataType`, the two values mini-Sowel uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    Boolean,
    Number,
}

/// Sowel's `Device`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: Id,
    pub integration_id: Cow<'static, str>,
    /// The EUI64 as `0x` and 16 lowercase hexadecimal digits (Zigbee2MQTT's form).
    pub source_device_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ieee_address: Option<String>,
    /// `null` until zones exist.
    pub zone_id: Option<String>,
    pub source: Cow<'static, str>,
    pub status: DeviceStatus,
    pub power_source: PowerSource,
    /// Milliseconds since 1970; written as an ISO 8601 date.
    #[serde(with = "iso_option")]
    pub last_seen: Option<u64>,
    #[serde(with = "iso")]
    pub created_at: u64,
    #[serde(with = "iso")]
    pub updated_at: u64,
}

/// Sowel's `DeviceData`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub id: Id,
    pub device_id: Id,
    pub key: Cow<'static, str>,
    #[serde(rename = "type")]
    pub value_type: ValueType,
    pub category: Cow<'static, str>,
    /// `null` until the first value.
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<Cow<'static, str>>,
    #[serde(with = "iso_option")]
    pub last_updated: Option<u64>,
}

/// Sowel's `DeviceOrder`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOrder {
    pub id: Id,
    pub device_id: Id,
    pub key: Cow<'static, str>,
    #[serde(rename = "type")]
    pub value_type: ValueType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Cow<'static, str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_on: Option<Cow<'static, str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_off: Option<Cow<'static, str>>,
}

/// Sowel's `DeviceWithDetails`: a device with its data and orders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceWithDetails {
    #[serde(flatten)]
    pub device: Device,
    pub data: Vec<DeviceData>,
    pub orders: Vec<DeviceOrder>,
}
