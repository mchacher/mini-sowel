//! The data of a zone seen from its equipments: Sowel's `ZoneAggregatedData` (T:283) and the
//! rules of `src/zones/zone-aggregator.ts`. A zone counts its own equipments and those of all
//! the zones below it. An equipment whose status is `offline` is skipped and listed as
//! unavailable; a `degraded` one still counts.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Serialize, Serializer};

use super::Zones;
use crate::devices::{Devices, Id, iso_option, json_number};
use crate::equipments::{
    EquipmentStatus, EquipmentType, Equipments, Index, StoredEquipment, status_with,
};

/// Sowel's `ZoneAggregatedData`. The fields of what Solite has no equipment for (doors,
/// valves, power, displays, sunlight) are present with their empty value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneAggregatedData {
    #[serde(serialize_with = "number")]
    pub temperature: Option<f64>,
    #[serde(serialize_with = "number")]
    pub humidity: Option<f64>,
    pub luminosity: Option<i64>,
    pub motion: bool,
    pub motion_sensors: u32,
    /// Set by the home, which remembers the last motion of each zone.
    #[serde(with = "iso_option")]
    pub motion_since: Option<u64>,
    pub open_doors: u32,
    pub open_windows: u32,
    pub water_leak: bool,
    pub smoke: bool,
    pub lights_on: u32,
    pub lights_total: u32,
    pub shutters_open: u32,
    pub shutters_total: u32,
    pub average_shutter_position: Option<i64>,
    pub doors_open: u32,
    pub doors_total: u32,
    pub water_valves_open: u32,
    pub water_valves_total: u32,
    pub water_flow_total: Option<i64>,
    pub power_total: Option<i64>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub is_daylight: Option<bool>,
    pub displays_online: u32,
    pub displays_total: u32,
    pub unavailable_equipments_by_category: BTreeMap<String, usize>,
    pub unavailable_equipment_names_by_category: BTreeMap<String, Vec<String>>,
}

/// A whole number without a decimal point (`20`, not `20.0`), as JavaScript writes it.
fn number<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(v) => json_number(*v).serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// JavaScript's `Math.round`: halves go up.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// What the equipments of one zone add up to; zones add up their children's.
#[derive(Debug, Default, Clone)]
struct Sum {
    temperature: f64,
    temperatures: u32,
    humidity: f64,
    humidities: u32,
    luminosity: f64,
    luminosities: u32,
    motion: bool,
    motion_sensors: u32,
    open_doors: u32,
    open_windows: u32,
    water_leak: bool,
    smoke: bool,
    lights_on: u32,
    lights_total: u32,
    shutter_position: f64,
    shutter_positions: u32,
    shutters_open: u32,
    shutters_total: u32,
    unavailable: BTreeMap<String, Vec<String>>,
}

impl Sum {
    fn add(&mut self, other: &Sum) {
        self.temperature += other.temperature;
        self.temperatures += other.temperatures;
        self.humidity += other.humidity;
        self.humidities += other.humidities;
        self.luminosity += other.luminosity;
        self.luminosities += other.luminosities;
        self.motion |= other.motion;
        self.motion_sensors += other.motion_sensors;
        self.open_doors += other.open_doors;
        self.open_windows += other.open_windows;
        self.water_leak |= other.water_leak;
        self.smoke |= other.smoke;
        self.lights_on += other.lights_on;
        self.lights_total += other.lights_total;
        self.shutter_position += other.shutter_position;
        self.shutter_positions += other.shutter_positions;
        self.shutters_open += other.shutters_open;
        self.shutters_total += other.shutters_total;
        for (category, names) in &other.unavailable {
            self.unavailable
                .entry(category.clone())
                .or_default()
                .extend(names.iter().cloned());
        }
    }

    fn data(&self) -> ZoneAggregatedData {
        let mean = |sum: f64, count: u32| (count > 0).then(|| sum / f64::from(count));
        let names: BTreeMap<String, Vec<String>> = self
            .unavailable
            .iter()
            .map(|(category, names)| {
                let mut names = names.clone();
                names.sort_by(
                    |a, b| // JavaScript's `localeCompare`: letters regardless of case, and for the same letters
                    // the lowercase first ("a" < "A" < "b").
                    a.to_lowercase().cmp(&b.to_lowercase()).then(b.cmp(a)),
                );
                (category.clone(), names)
            })
            .collect();
        ZoneAggregatedData {
            temperature: mean(self.temperature, self.temperatures)
                .map(|m| js_round(m * 10.0) / 10.0),
            humidity: mean(self.humidity, self.humidities).map(|m| js_round(m * 10.0) / 10.0),
            luminosity: mean(self.luminosity, self.luminosities).map(|m| js_round(m) as i64),
            motion: self.motion,
            motion_sensors: self.motion_sensors,
            motion_since: None,
            open_doors: self.open_doors,
            open_windows: self.open_windows,
            water_leak: self.water_leak,
            smoke: self.smoke,
            lights_on: self.lights_on,
            lights_total: self.lights_total,
            shutters_open: self.shutters_open,
            shutters_total: self.shutters_total,
            average_shutter_position: mean(self.shutter_position, self.shutter_positions)
                .map(|m| js_round(m) as i64),
            doors_open: 0,
            doors_total: 0,
            water_valves_open: 0,
            water_valves_total: 0,
            water_flow_total: None,
            power_total: None,
            sunrise: None,
            sunset: None,
            is_daylight: None,
            displays_online: 0,
            displays_total: 0,
            unavailable_equipments_by_category: names
                .iter()
                .map(|(category, names)| (category.clone(), names.len()))
                .collect(),
            unavailable_equipment_names_by_category: names,
        }
    }
}

/// `true` or `"ON"` (Sowel's `isBooleanActive`).
fn active(value: &serde_json::Value) -> bool {
    value.as_bool() == Some(true) || value.as_str() == Some("ON")
}

/// `false` or `"OFF"`: a contact that is open (Sowel's `isContactOpen`).
fn open(value: &serde_json::Value) -> bool {
    value.as_bool() == Some(false) || value.as_str() == Some("OFF")
}

/// The categories an offline equipment is missing from, once each (`unavailableCategories`):
/// the rules the online totals use, so the root never shows an unavailable light that no child
/// zone can account for.
fn unavailable_categories(index: &Index<'_>, item: &StoredEquipment) -> BTreeSet<String> {
    let mut categories = BTreeSet::new();
    for binding in &item.data_bindings {
        let Some((_, data)) = index.resolve(binding) else {
            continue;
        };
        let category: &str = &data.category;
        if category == "light_state" && !item.equipment.kind.is_light() {
            continue;
        }
        if category == "shutter_position" && item.equipment.kind != EquipmentType::Shutter {
            continue;
        }
        if category == "temperature" && binding.alias != "temperature" {
            continue;
        }
        categories.insert(category.to_string());
    }
    categories
}

/// What one equipment adds (`accumulateBindings`).
fn accumulate(sum: &mut Sum, index: &Index<'_>, item: &StoredEquipment) {
    let kind = item.equipment.kind;
    // A light counts once, per equipment: on when any of its channels is on.
    let mut has_light_state = false;
    let mut light_on = false;
    for binding in &item.data_bindings {
        let Some((_, data)) = index.resolve(binding) else {
            continue;
        };
        let value = &data.value;
        match &*data.category {
            "temperature" => {
                if let (Some(v), true) = (value.as_f64(), binding.alias == "temperature") {
                    sum.temperature += v;
                    sum.temperatures += 1;
                }
            }
            "humidity" => {
                if let Some(v) = value.as_f64() {
                    sum.humidity += v;
                    sum.humidities += 1;
                }
            }
            "luminosity" => {
                if let Some(v) = value.as_f64() {
                    sum.luminosity += v;
                    sum.luminosities += 1;
                }
            }
            "motion" => {
                sum.motion_sensors += 1;
                sum.motion |= active(value);
            }
            "contact_door" => sum.open_doors += u32::from(open(value)),
            "contact_window" => sum.open_windows += u32::from(open(value)),
            "water_leak" => sum.water_leak |= active(value),
            "smoke" => sum.smoke |= active(value),
            "light_state" if kind.is_light() => {
                has_light_state = true;
                light_on |= active(value);
            }
            "shutter_position" if kind == EquipmentType::Shutter => {
                sum.shutters_total += 1;
                if let Some(v) = value.as_f64() {
                    sum.shutter_position += v;
                    sum.shutter_positions += 1;
                    sum.shutters_open += u32::from(v > 0.0);
                }
            }
            _ => {}
        }
    }
    if has_light_state {
        sum.lights_total += 1;
        sum.lights_on += u32::from(light_on);
    }
}

/// The aggregated data of every zone, with `motion_since` empty (the home fills it in).
pub fn compute(
    zones: &Zones,
    equipments: &Equipments,
    devices: &Devices,
    now: u64,
) -> Vec<(Id, ZoneAggregatedData)> {
    // The data behind the bindings, found by id in one lookup each.
    let index = Index::new(devices);
    // The zone's own equipments.
    let mut own: Vec<(Id, Sum)> = zones
        .list()
        .iter()
        .map(|z| (z.id, Sum::default()))
        .collect();
    for item in equipments.list() {
        let Some((_, sum)) = own.iter_mut().find(|(id, _)| *id == item.equipment.zone_id) else {
            continue;
        };
        let (status, _) = status_with(&index, item, now);
        if status == EquipmentStatus::Offline {
            for category in unavailable_categories(&index, item) {
                sum.unavailable
                    .entry(category)
                    .or_default()
                    .push(item.equipment.name.to_string());
            }
            continue;
        }
        accumulate(sum, &index, item);
    }
    // Each zone adds up its own and those of the zones below it.
    own.iter()
        .map(|(id, _)| {
            let mut total = Sum::default();
            for (other, sum) in &own {
                if zones.is_within(other, id) {
                    total.add(sum);
                }
            }
            (*id, total.data())
        })
        .collect()
}
