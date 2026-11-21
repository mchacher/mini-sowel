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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id([u8; 16]);

impl Id {
    /// The fixed id of the root zone, `00000000-0000-0000-0000-000000000001`
    /// (`ROOT_ZONE_ID` in Sowel's `src/shared/constants.ts`).
    pub const ROOT_ZONE: Id = Id([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

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
pub(crate) mod iso {
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
pub(crate) mod iso_option {
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

/// Sowel's `Device`. Memory: strings are exact-size `Box<str>`; what is the same for every device
/// (`integrationId`, `source`) or equal to another field (`ieeeAddress` is the `sourceDeviceId`)
/// is not stored but written when the device is serialised.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: Id,
    /// The EUI64 as `0x` and 16 lowercase hexadecimal digits (Zigbee2MQTT's form).
    pub source_device_id: Box<str>,
    pub name: Box<str>,
    pub manufacturer: Option<Box<str>>,
    pub model: Option<Box<str>>,
    /// The zone the device is in, `None` when it is in none (`zoneId: null`).
    pub zone_id: Option<Id>,
    pub status: DeviceStatus,
    pub power_source: PowerSource,
    /// Milliseconds since 1970; written as an ISO 8601 date.
    pub last_seen: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceOut<'a> {
    id: Id,
    integration_id: &'static str,
    source_device_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    manufacturer: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    ieee_address: &'a str,
    zone_id: Option<Id>,
    source: &'static str,
    status: DeviceStatus,
    power_source: PowerSource,
    #[serde(with = "iso_option")]
    last_seen: Option<u64>,
    #[serde(with = "iso")]
    created_at: u64,
    #[serde(with = "iso")]
    updated_at: u64,
}

impl Serialize for Device {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DeviceOut {
            id: self.id,
            integration_id: INTEGRATION_ID,
            source_device_id: &self.source_device_id,
            name: &self.name,
            manufacturer: self.manufacturer.as_deref(),
            model: self.model.as_deref(),
            ieee_address: &self.source_device_id,
            zone_id: self.zone_id,
            source: INTEGRATION_ID,
            status: self.status,
            power_source: self.power_source,
            last_seen: self.last_seen,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
        .serialize(serializer)
    }
}

/// What is read of a device: `integrationId`, `source` and `ieeeAddress` are ignored.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceIn {
    id: Id,
    source_device_id: Box<str>,
    name: Box<str>,
    #[serde(default)]
    manufacturer: Option<Box<str>>,
    #[serde(default)]
    model: Option<Box<str>>,
    zone_id: Option<Id>,
    status: DeviceStatus,
    power_source: PowerSource,
    #[serde(with = "iso_option")]
    last_seen: Option<u64>,
    #[serde(with = "iso")]
    created_at: u64,
    #[serde(with = "iso")]
    updated_at: u64,
}

impl<'de> Deserialize<'de> for Device {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Device, D::Error> {
        let d = DeviceIn::deserialize(deserializer)?;
        Ok(Device {
            id: d.id,
            source_device_id: d.source_device_id,
            name: d.name,
            manufacturer: d.manufacturer,
            model: d.model,
            zone_id: d.zone_id,
            status: d.status,
            power_source: d.power_source,
            last_seen: d.last_seen,
            created_at: d.created_at,
            updated_at: d.updated_at,
        })
    }
}

/// Sowel's `DeviceData`, without its `deviceId`: the device that holds it knows it, and writes it
/// (`DataOut`) when it is serialised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub id: Id,
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
    /// The last time the value changed, not just was received (the equipment's
    /// `lastChanged`). Kept in `devices.json`, left out of the device API: Sowel's `DeviceData`
    /// has no such field.
    #[serde(skip)]
    pub last_changed: Option<u64>,
}

/// Sowel's `DeviceOrder`, without its `deviceId` (see `DeviceData`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOrder {
    pub id: Id,
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

/// A data with the `deviceId` of its device, as Sowel writes it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataOut<'a> {
    pub device_id: Id,
    #[serde(flatten)]
    pub data: &'a DeviceData,
}

/// An order with the `deviceId` of its device.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderOut<'a> {
    pub device_id: Id,
    #[serde(flatten)]
    pub order: &'a DeviceOrder,
}

/// The data of a device, each with the `deviceId`.
struct DataList<'a>(Id, &'a [DeviceData]);

impl Serialize for DataList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.1.iter().map(|data| DataOut {
            device_id: self.0,
            data,
        }))
    }
}

/// The orders of a device, each with the `deviceId`.
pub(crate) struct OrderList<'a>(pub Id, pub &'a [DeviceOrder]);

impl Serialize for OrderList<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.1.iter().map(|order| OrderOut {
            device_id: self.0,
            order,
        }))
    }
}

/// How to reach a device on the network (spec 011): the node address and the endpoint its On/Off
/// and Level Control frames come from, as the last messages told. Kept in `devices.json` beside
/// Sowel's fields, never in the device API.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Route {
    pub nwk: Option<u16>,
    /// `None` until a frame tells it: endpoint 1 is then used.
    pub endpoint: Option<u8>,
}

/// Sowel's `DeviceWithDetails`: a device with its data and orders.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceWithDetails {
    pub device: Device,
    pub data: Vec<DeviceData>,
    pub orders: Vec<DeviceOrder>,
    /// Not Sowel's: how to send the device an order.
    pub route: Route,
}

#[derive(Serialize)]
struct DetailsOut<'a> {
    #[serde(flatten)]
    device: &'a Device,
    data: DataList<'a>,
    orders: OrderList<'a>,
}

impl Serialize for DeviceWithDetails {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let id = self.device.id;
        DetailsOut {
            device: &self.device,
            data: DataList(id, &self.data),
            orders: OrderList(id, &self.orders),
        }
        .serialize(serializer)
    }
}

#[derive(Deserialize)]
struct DetailsIn {
    #[serde(flatten)]
    device: Device,
    data: Vec<DeviceData>,
    orders: Vec<DeviceOrder>,
}

impl<'de> Deserialize<'de> for DeviceWithDetails {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let d = DetailsIn::deserialize(deserializer)?;
        Ok(DeviceWithDetails {
            device: d.device,
            data: d.data,
            orders: d.orders,
            route: Route::default(),
        })
    }
}
