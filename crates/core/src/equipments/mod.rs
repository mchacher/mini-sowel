//! Equipments (spec 010): Sowel's `Equipment`, `DataBinding` and `OrderBinding`
//! (`src/shared/types.ts` T:419, T:523, T:576) with the rules of `src/equipments/
//! equipment-manager.ts`. An equipment binds device data and orders under aliases; recipes, the
//! interface and the zone aggregation read equipments, never devices. No I/O: the caller writes.

mod status;
mod view;

use std::ops::Deref;
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::devices::{Id, iso};

pub use status::{EquipmentStatus, StatusReason, is_stale, stale_window_ms};
pub use view::status_of;
pub use view::{Details, details};
pub(crate) use view::{Index, status_with};

/// Equipments kept; one past it is refused (400).
pub const MAX_EQUIPMENTS: usize = 64;
/// Data bindings of one equipment, and order bindings of one equipment; one past it is refused
/// (400), or skipped when the equipment is created from devices.
pub const MAX_BINDINGS: usize = 16;
/// The longest alias, in characters (Sowel sets none).
pub const MAX_ALIAS_CHARS: usize = 32;

/// Sowel's `EquipmentType`, the seven values of decision D3's six kinds (a light has two).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentType {
    LightOnoff,
    LightDimmable,
    Shutter,
    Sensor,
    Switch,
    Thermostat,
    Button,
}

impl EquipmentType {
    pub fn parse(text: &str) -> Option<EquipmentType> {
        Some(match text {
            "light_onoff" => EquipmentType::LightOnoff,
            "light_dimmable" => EquipmentType::LightDimmable,
            "shutter" => EquipmentType::Shutter,
            "sensor" => EquipmentType::Sensor,
            "switch" => EquipmentType::Switch,
            "thermostat" => EquipmentType::Thermostat,
            "button" => EquipmentType::Button,
            _ => return None,
        })
    }

    /// The type as Sowel writes it (`light_onoff`).
    pub fn as_str(self) -> &'static str {
        match self {
            EquipmentType::LightOnoff => "light_onoff",
            EquipmentType::LightDimmable => "light_dimmable",
            EquipmentType::Shutter => "shutter",
            EquipmentType::Sensor => "sensor",
            EquipmentType::Switch => "switch",
            EquipmentType::Thermostat => "thermostat",
            EquipmentType::Button => "button",
        }
    }

    pub fn is_light(self) -> bool {
        matches!(
            self,
            EquipmentType::LightOnoff | EquipmentType::LightDimmable
        )
    }
}

/// An alias of a binding (`state`, `temperature`...). The same few words come back in every
/// equipment, so they are shared: each distinct alias is stored once (the table is bounded; past
/// it an alias is simply not shared).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias(Arc<str>);

/// Aliases shared so far, at most `MAX_SHARED_ALIASES`.
static SHARED: Mutex<Vec<Arc<str>>> = Mutex::new(Vec::new());
const MAX_SHARED_ALIASES: usize = 128;

impl Alias {
    pub fn new(text: &str) -> Alias {
        let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = shared.iter().find(|a| &***a == text) {
            return Alias(Arc::clone(found));
        }
        let new: Arc<str> = Arc::from(text);
        if shared.len() < MAX_SHARED_ALIASES {
            shared.push(Arc::clone(&new));
        }
        Alias(new)
    }
}

impl Deref for Alias {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for Alias {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<&str> for Alias {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}

impl Serialize for Alias {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Alias {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Alias, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Alias::new(&text))
    }
}

/// Sowel's `Equipment`; its optional flags (energy, timed actions...) are left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Equipment {
    pub id: Id,
    pub name: Box<str>,
    pub zone_id: Id,
    #[serde(rename = "type")]
    pub kind: EquipmentType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Box<str>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Box<str>>,
    pub enabled: bool,
    #[serde(with = "iso")]
    pub created_at: u64,
    #[serde(with = "iso")]
    pub updated_at: u64,
}

/// Sowel's `DataBinding` (`historize` is left out), without its `equipmentId`: the equipment
/// that holds it knows it and writes it when it is serialised (`DataBindingOut`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataBinding {
    pub id: Id,
    pub device_data_id: Id,
    pub alias: Alias,
}

/// Sowel's `OrderBinding`, without its `equipmentId` (see `DataBinding`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBinding {
    pub id: Id,
    pub device_order_id: Id,
    pub alias: Alias,
}

/// A data binding with the `equipmentId` of its equipment, as Sowel writes it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataBindingOut<'a> {
    pub equipment_id: Id,
    #[serde(flatten)]
    pub binding: &'a DataBinding,
}

/// An order binding with the `equipmentId` of its equipment.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBindingOut<'a> {
    pub equipment_id: Id,
    #[serde(flatten)]
    pub binding: &'a OrderBinding,
}

struct DataBindings<'a>(Id, &'a [DataBinding]);

impl Serialize for DataBindings<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.1.iter().map(|binding| DataBindingOut {
            equipment_id: self.0,
            binding,
        }))
    }
}

struct OrderBindings<'a>(Id, &'a [OrderBinding]);

impl Serialize for OrderBindings<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.1.iter().map(|binding| OrderBindingOut {
            equipment_id: self.0,
            binding,
        }))
    }
}

/// An equipment with its bindings, as `equipments.json` keeps it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEquipment {
    #[serde(flatten)]
    pub equipment: Equipment,
    /// At most `MAX_BINDINGS`, aliases distinct.
    pub data_bindings: Vec<DataBinding>,
    /// At most `MAX_BINDINGS`, aliases distinct.
    pub order_bindings: Vec<OrderBinding>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredOut<'a> {
    #[serde(flatten)]
    equipment: &'a Equipment,
    data_bindings: DataBindings<'a>,
    order_bindings: OrderBindings<'a>,
}

impl Serialize for StoredEquipment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let id = self.equipment.id;
        StoredOut {
            equipment: &self.equipment,
            data_bindings: DataBindings(id, &self.data_bindings),
            order_bindings: OrderBindings(id, &self.order_bindings),
        }
        .serialize(serializer)
    }
}

/// The equipments, at most `MAX_EQUIPMENTS`.
#[derive(Debug, Default)]
pub struct Equipments {
    items: Vec<StoredEquipment>,
}

impl Equipments {
    pub fn new() -> Self {
        Equipments::default()
    }

    /// The equipments read from storage, at most `MAX_EQUIPMENTS`, each with at most
    /// `MAX_BINDINGS` bindings of each kind and one binding per alias. The consistency with the
    /// zones and the devices is the home's (`Home::from_parts`).
    pub fn from_list(mut list: Vec<StoredEquipment>) -> Self {
        list.truncate(MAX_EQUIPMENTS);
        for item in &mut list {
            item.data_bindings.truncate(MAX_BINDINGS);
            item.order_bindings.truncate(MAX_BINDINGS);
            item.data_bindings.shrink_to_fit();
            item.order_bindings.shrink_to_fit();
        }
        list.shrink_to_fit();
        Equipments { items: list }
    }

    pub fn list(&self) -> &[StoredEquipment] {
        &self.items
    }

    pub fn list_mut(&mut self) -> &mut Vec<StoredEquipment> {
        &mut self.items
    }

    pub fn get(&self, id: &str) -> Option<&StoredEquipment> {
        let id = Id::parse(id)?;
        self.items.iter().find(|e| e.equipment.id == id)
    }

    pub fn get_by_id(&self, id: &Id) -> Option<&StoredEquipment> {
        self.items.iter().find(|e| e.equipment.id == *id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut StoredEquipment> {
        let id = Id::parse(id)?;
        self.items.iter_mut().find(|e| e.equipment.id == id)
    }

    /// How many equipments are in this zone.
    pub fn count_in_zone(&self, zone: &Id) -> usize {
        self.items
            .iter()
            .filter(|e| e.equipment.zone_id == *zone)
            .count()
    }

    /// The equipments as the JSON list that is saved.
    pub fn to_json(&self) -> Result<String, String> {
        crate::devices::to_json(&self.items).ok_or_else(|| "cannot serialise the equipments".into())
    }
}

/// The equipments read back from the JSON `Equipments::to_json` wrote.
pub fn parse_list(json: &str) -> Result<Vec<StoredEquipment>, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}
