//! `NativeHost`: the recipe SDK's `Host` over the home (spec 041, R3.2). It is built for one
//! call of one instance, with the call's clocks; the recipe reads the home through it, sends its
//! orders through the home's queue of recipe orders (as `OrderSource::Recipe`) and writes its
//! log lines into it, in the order they are written. Script recipes reach the home through this
//! same code (the functions the shim calls).

use std::cell::RefCell;
use std::sync::Arc;

use serde_json::Value;
use solite_recipe_sdk::bounds::MAX_ZONE_EQUIPMENT_IDS;
use solite_recipe_sdk::{
    DataBindingView, EquipmentView, EventValue, Host, Level, OrderBindingView, OrderStatus,
    Sunlight, ZoneData,
};

use crate::devices::Id;
use crate::equipments::StoredEquipment;
use crate::home::Home;
use crate::orders::{OrderRequest, OrderSource, commands_for};

/// The recipe an instance belongs to, as its orders name it (shared with the engine's entry).
#[derive(Debug, Clone)]
pub struct RecipeName {
    pub id: Arc<str>,
    pub name: Arc<str>,
}

pub struct NativeHost<'a> {
    home: &'a Home,
    instance: Id,
    recipe: &'a RecipeName,
    mono: u64,
    wall: u64,
    /// The lines the recipe logged during the call, in order; the engine takes them after it.
    /// Bounded by what one call logs (a few lines).
    lines: RefCell<Vec<(Level, String)>>,
}

impl<'a> NativeHost<'a> {
    pub fn new(
        home: &'a Home,
        instance: Id,
        recipe: &'a RecipeName,
        mono: u64,
        wall: u64,
    ) -> NativeHost<'a> {
        NativeHost {
            home,
            instance,
            recipe,
            mono,
            wall,
            lines: RefCell::new(Vec::new()),
        }
    }

    /// The lines logged so far; the host is left with none.
    pub fn take_lines(&self) -> Vec<(Level, String)> {
        std::mem::take(&mut *self.lines.borrow_mut())
    }

    fn view(&self, item: &StoredEquipment) -> EquipmentView {
        let devices = &self.home.devices;
        EquipmentView {
            id: item.equipment.id.to_string(),
            name: item.equipment.name.to_string(),
            kind: item.equipment.kind.as_str().to_string(),
            enabled: item.equipment.enabled,
            zone_id: item.equipment.zone_id.to_string(),
            data_bindings: item
                .data_bindings
                .iter()
                .map(|binding| {
                    let data = devices.data_of(&binding.device_data_id);
                    DataBindingView {
                        id: binding.id.to_string(),
                        alias: binding.alias.to_string(),
                        category: data.map(|(_, d)| d.category.to_string()),
                        device_id: data.map(|(device, _)| device.id.to_string()),
                        key: data.map(|(_, d)| d.key.to_string()),
                        value: data
                            .map_or(EventValue::Null, |(_, d)| EventValue::from_json(&d.value)),
                    }
                })
                .collect(),
            order_bindings: item
                .order_bindings
                .iter()
                .map(|binding| OrderBindingView {
                    id: binding.id.to_string(),
                    alias: binding.alias.to_string(),
                    category: devices
                        .order_of(&binding.device_order_id)
                        .and_then(|(_, order)| order.category.as_deref().map(str::to_string)),
                })
                .collect(),
        }
    }
}

impl Host for NativeHost<'_> {
    fn equipment(&self, id: &str) -> Option<EquipmentView> {
        let item = self.home.equipments().get(id)?;
        Some(self.view(item))
    }

    fn zone_equipment_ids(&self, zone: &str) -> Vec<String> {
        let Some(zone) = Id::parse(zone) else {
            return Vec::new();
        };
        self.home
            .equipments()
            .list()
            .iter()
            .filter(|item| {
                item.equipment.enabled
                    && self.home.zones().is_within(&item.equipment.zone_id, &zone)
            })
            .map(|item| item.equipment.id.to_string())
            .take(MAX_ZONE_EQUIPMENT_IDS)
            .collect()
    }

    fn zone_data(&self, zone: &str) -> Option<ZoneData> {
        let zone = Id::parse(zone)?;
        let (motion, luminosity) = self.home.zone_reading(&zone, self.wall)?;
        let motion_sensors = self
            .home
            .zone_aggregate(&zone, self.wall)
            .map_or(0, |data| data.motion_sensors);
        Some(ZoneData {
            motion,
            luminosity,
            motion_sensors,
        })
    }

    fn sunlight(&self) -> Sunlight {
        let sun = self.home.sunlight(self.wall);
        Sunlight {
            is_daylight: sun.is_daylight,
            sunrise: sun.sunrise,
            sunset: sun.sunset,
        }
    }

    fn utc_offset_seconds(&self, utc_ms: u64) -> i64 {
        self.home
            .settings()
            .time_zone()
            .offset_at(i64::try_from(utc_ms).unwrap_or(i64::MAX))
    }

    fn order(&self, equipment: &str, binding: &str, value: &Value) -> OrderStatus {
        let Some(item) = self.home.equipments().get(equipment) else {
            return OrderStatus::NoBinding;
        };
        let Some(binding) =
            Id::parse(binding).and_then(|id| item.order_bindings.iter().find(|b| b.id == id))
        else {
            return OrderStatus::NoBinding;
        };
        if !item.equipment.enabled {
            return OrderStatus::Disabled;
        }
        let Ok(commands) = commands_for(
            &self.home.devices,
            std::slice::from_ref(binding),
            &binding.alias,
            value,
        ) else {
            return OrderStatus::Refused;
        };
        if commands.is_empty() {
            return OrderStatus::Refused;
        }
        let request = OrderRequest {
            equipment: item.equipment.id,
            alias: binding.alias.to_string(),
            value: value.clone(),
            source: Some(OrderSource::Recipe {
                instance_id: self.instance,
                recipe_name: Arc::clone(&self.recipe.name),
                recipe_id: Arc::clone(&self.recipe.id),
            }),
            commands,
        };
        if self
            .home
            .orders()
            .push_unattended(request, self.mono)
            .is_err()
        {
            log::warn!(
                "recipe order for {} dropped: the queue of recipe orders is full",
                item.equipment.id
            );
            return OrderStatus::QueueFull;
        }
        OrderStatus::Sent
    }

    fn log(&self, level: Level, message: &str) {
        self.lines.borrow_mut().push((level, message.to_string()));
    }
}
