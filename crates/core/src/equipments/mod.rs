//! Equipments (spec 010): Sowel's `Equipment`, `DataBinding` and `OrderBinding`
//! (`src/shared/types.ts` T:419, T:523, T:576) with the rules of `src/equipments/
//! equipment-manager.ts`. An equipment binds device data and orders under aliases; recipes, the
//! interface and the zone aggregation read equipments, never devices. No I/O: the caller writes.

mod extras;
pub mod propose;
pub mod rules;
mod status;
mod status_tracker;
mod view;

use std::ops::Deref;
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::devices::{Id, iso};

pub use extras::{
    EquipmentFamily, Extras, Field, MAX_EXTRAS_BYTES, MAX_EXTRAS_TOTAL_BYTES, MAX_ID_CHARS, Piece,
    RawObject,
};
pub use propose::{Proposal, propose};
pub use status::{EquipmentStatus, StatusReason, is_stale, stale_window_ms};
pub use status_tracker::{StatusTracker, Transition};
pub use view::status_of;
pub use view::{Details, details};
pub(crate) use view::{Index, status_with};

/// Equipments kept; one past it is refused (400).
pub const MAX_EQUIPMENTS: usize = 64;
/// Data bindings of one equipment, and order bindings of one equipment; one past it is refused
/// (400), or skipped when the equipment is created from devices.
pub const MAX_BINDINGS: usize = 8;
/// The longest alias, in characters (Sowel sets none).
pub const MAX_ALIAS_CHARS: usize = 32;

/// Defines `EquipmentType` from one list of (variant, Sowel's name), so the enum, `ALL`, `as_str`
/// and `parse` cannot disagree.
macro_rules! equipment_types {
    ($($variant:ident => $name:literal,)+) => {
        /// Sowel's `EquipmentType` (`src/shared/types.ts` T:339): its 31 values, written as Sowel
        /// writes them. Eight have binding rules (`rules.rs`); the others are accepted, stored
        /// and served.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum EquipmentType {
            $(
                #[serde(rename = $name)]
                $variant,
            )+
        }

        impl EquipmentType {
            /// Every value, in Sowel's order.
            pub const ALL: [EquipmentType; 31] = [$(EquipmentType::$variant),+];

            pub fn parse(text: &str) -> Option<EquipmentType> {
                match text {
                    $($name => Some(EquipmentType::$variant),)+
                    _ => None,
                }
            }

            /// The type as Sowel writes it (`light_onoff`).
            pub fn as_str(self) -> &'static str {
                match self {
                    $(EquipmentType::$variant => $name,)+
                }
            }
        }
    };
}

equipment_types! {
    LightOnoff => "light_onoff",
    LightDimmable => "light_dimmable",
    LightColor => "light_color",
    Shutter => "shutter",
    Awning => "awning",
    Door => "door",
    Switch => "switch",
    Sensor => "sensor",
    Button => "button",
    Thermostat => "thermostat",
    Weather => "weather",
    WeatherForecast => "weather_forecast",
    Gate => "gate",
    Heater => "heater",
    WaterHeater => "water_heater",
    EnergyMeter => "energy_meter",
    MainEnergyMeter => "main_energy_meter",
    EnergyProductionMeter => "energy_production_meter",
    SolarPanel => "solar_panel",
    MediaPlayer => "media_player",
    Appliance => "appliance",
    WaterValve => "water_valve",
    PoolPump => "pool_pump",
    PoolCover => "pool_cover",
    PoolHeatPump => "pool_heat_pump",
    Display => "display",
    Camera => "camera",
    Vmc => "vmc",
    Ups => "ups",
    EvCharger => "ev_charger",
    ElectricVehicle => "electric_vehicle",
}

impl EquipmentType {
    /// Sowel's `LIGHT_EQUIPMENT_TYPES` (`src/zones/zone-aggregator.ts`): what counts as a light
    /// in the zone aggregation.
    pub fn is_light(self) -> bool {
        matches!(
            self,
            EquipmentType::LightOnoff | EquipmentType::LightDimmable | EquipmentType::LightColor
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

/// Sowel's `Equipment`, with its twelve optional fields kept in `extras` (spec 027 R2.3): an
/// equipment without them pays 8 bytes. `StoredEquipment` reads it from `equipments.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Sowel's optional fields (`requireConfirmation`, `family`...), written flat in the JSON.
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub extras: Option<Box<Extras>>,
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
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEquipment {
    pub equipment: Equipment,
    /// At most `MAX_BINDINGS`, aliases distinct.
    pub data_bindings: Vec<DataBinding>,
    /// At most `MAX_BINDINGS`, aliases distinct.
    pub order_bindings: Vec<OrderBinding>,
}

/// The fields of an equipment but its twelve optional ones (`RawExtras` reads those).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredIn {
    id: Id,
    name: Box<str>,
    zone_id: Id,
    #[serde(rename = "type")]
    kind: EquipmentType,
    #[serde(default)]
    icon: Option<Box<str>>,
    #[serde(default)]
    description: Option<Box<str>>,
    enabled: bool,
    #[serde(with = "iso")]
    created_at: u64,
    #[serde(with = "iso")]
    updated_at: u64,
    data_bindings: Vec<DataBinding>,
    order_bindings: Vec<OrderBinding>,
}

impl<'de> Deserialize<'de> for StoredEquipment {
    /// Read in two passes over the element's text: `RawValue` (the objects of the optional
    /// fields, kept as written) does not work through `flatten`. A field of the wrong shape is
    /// dropped with a warning, the rest of the equipment is kept; bounds: `Equipments::from_list`.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let text = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        let core: StoredIn = serde_json::from_str(text.get()).map_err(D::Error::custom)?;
        let raw: extras::RawExtras = serde_json::from_str(text.get()).map_err(D::Error::custom)?;
        let extras = Extras::from_raw(&raw, &core.name);
        Ok(StoredEquipment {
            equipment: Equipment {
                id: core.id,
                name: core.name,
                zone_id: core.zone_id,
                kind: core.kind,
                icon: core.icon,
                description: core.description,
                enabled: core.enabled,
                created_at: core.created_at,
                updated_at: core.updated_at,
                extras,
            },
            data_bindings: core.data_bindings,
            order_bindings: core.order_bindings,
        })
    }
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

    /// The equipments read from storage, at most `MAX_EQUIPMENTS`. Their bindings are **not** cut
    /// here: `Home::from_parts`, the only caller, first drops the dangling ones, then keeps the
    /// first `MAX_BINDINGS` of each kind and one binding per alias, so a valid binding is never
    /// lost to a dropped one. Crate-only so that no other path holds the bindings uncut. Memory:
    /// the lists read are bounded by the file's size and cut right after.
    pub(crate) fn from_list(mut list: Vec<StoredEquipment>) -> Self {
        list.truncate(MAX_EQUIPMENTS);
        // The optional fields past their bounds are dropped, in the file's order.
        let mut used = 0;
        for item in &mut list {
            if let Some(extras) = item.equipment.extras.take() {
                item.equipment.extras = extras.bounded(&item.equipment.name, &mut used);
            }
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

    /// The bytes the optional fields of all equipments take, as written in JSON, but those of
    /// the equipment `except` (when it is given): what a change of that one is counted against.
    pub fn extras_bytes(&self, except: Option<&Id>) -> usize {
        self.items
            .iter()
            .filter(|e| Some(&e.equipment.id) != except)
            .filter_map(|e| e.equipment.extras.as_deref())
            .map(Extras::json_len)
            .sum()
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
