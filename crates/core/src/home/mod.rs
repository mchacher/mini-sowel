//! The home of mini-Sowel: its devices, zones and equipments together, behind the one lock the
//! host takes. The rules that touch more than one of them live here (a zone deleted frees its
//! devices, a device deleted frees the bindings made on it); each file has its change flag, so the
//! radio loop, the only writer, knows what to write. No I/O, no clock and no random source of its
//! own: the caller gives them.

mod cursor;
mod equipment_ops;

pub use cursor::{CHUNK_ITEMS, ListCursor};

use serde::{Serialize, Serializer};

use crate::devices::{Device, Devices, Id, Random};
use crate::equipments::{Alias, Equipments, StoredEquipment};
use crate::failure::{Failure, Outcome};
use crate::orders::OrderQueue;
use crate::zones::aggregate::{ZoneAggregatedData, compute};
use crate::zones::{NewZone, Zone, ZoneUpdate, Zones};

pub use equipment_ops::{EquipmentUpdate, NewEquipment};

/// A UUID v4 from the caller's random bytes.
pub(crate) fn new_id(random: Random) -> Outcome<Id> {
    let mut bytes = [0u8; 16];
    random(&mut bytes).map_err(|e| {
        log::error!("no random bytes: {e}");
        Failure::new(500, "Internal error")
    })?;
    Ok(Id::from_random(bytes))
}

/// The last `(motion, motionSince)` of a zone.
#[derive(Debug, Clone, Copy)]
struct Motion {
    zone: Id,
    motion: bool,
    since: Option<u64>,
}

/// Devices, zones and equipments.
#[derive(Debug)]
pub struct Home {
    /// Public: the radio loop feeds it, and it keeps its own change flags.
    pub devices: Devices,
    zones: Zones,
    equipments: Equipments,
    /// The last motion state of each zone, at most one per zone (so at most `MAX_ZONES`).
    motion: Vec<Motion>,
    zones_changed: bool,
    equipments_changed: bool,
    /// The orders waiting for the radio (spec 011); at most `MAX_ORDERS`.
    orders: OrderQueue,
}

impl Home {
    /// A first start: no device, no equipment, the root zone (to be written).
    pub fn new(now: u64) -> Self {
        Home::from_parts(Devices::new(), Vec::new(), Vec::new(), now)
    }

    /// A home with these devices, and the root zone.
    pub fn from_devices(devices: Devices, now: u64) -> Self {
        Home::from_parts(devices, Vec::new(), Vec::new(), now)
    }

    /// The home read from storage, made consistent (spec 010, R7.11): the root zone is created
    /// when missing and orphans go under it; a device in an unknown zone is in none; an equipment
    /// in an unknown zone goes under the root; a binding whose data or order is gone, or whose
    /// alias is already taken, is dropped. What was repaired is marked to be written.
    pub fn from_parts(
        mut devices: Devices,
        zones: Vec<Zone>,
        equipments: Vec<StoredEquipment>,
        now: u64,
    ) -> Self {
        let (zones, zones_repaired) = Zones::from_list(zones, now);
        devices.fix_zones(&|zone| zones.find(zone).is_some());
        let mut equipments = Equipments::from_list(equipments);
        let mut repaired = false;
        let mut seen: Vec<Id> = Vec::new();
        let before = equipments.list().len();
        equipments.list_mut().retain(|e| {
            if seen.contains(&e.equipment.id) {
                false
            } else {
                seen.push(e.equipment.id);
                true
            }
        });
        repaired |= equipments.list().len() != before;
        for item in equipments.list_mut() {
            if zones.get(&item.equipment.zone_id).is_none() {
                item.equipment.zone_id = Id::ROOT_ZONE;
                repaired = true;
            }
            let bindings = item.data_bindings.len() + item.order_bindings.len();
            let mut aliases: Vec<Alias> = Vec::new();
            item.data_bindings.retain(|b| {
                let keep =
                    devices.data_of(&b.device_data_id).is_some() && !aliases.contains(&b.alias);
                if keep {
                    aliases.push(b.alias.clone());
                }
                keep
            });
            let mut aliases: Vec<Alias> = Vec::new();
            item.order_bindings.retain(|b| {
                let keep =
                    devices.order_of(&b.device_order_id).is_some() && !aliases.contains(&b.alias);
                if keep {
                    aliases.push(b.alias.clone());
                }
                keep
            });
            repaired |= bindings != item.data_bindings.len() + item.order_bindings.len();
        }
        Home {
            devices,
            zones,
            equipments,
            motion: Vec::new(),
            zones_changed: zones_repaired,
            equipments_changed: repaired,
            orders: OrderQueue::new(),
        }
    }

    pub fn zones(&self) -> &Zones {
        &self.zones
    }

    pub fn equipments(&self) -> &Equipments {
        &self.equipments
    }

    /// The queue of orders for the radio: the API gives it the orders it receives, and the core
    /// its own (a recipe) the same way; the radio loop takes them from a clone of this handle.
    pub fn orders(&self) -> &OrderQueue {
        &self.orders
    }

    // ---- What is to be written ----

    /// The zones as JSON if they changed since they were last taken, and the flag cleared. The
    /// caller writes it, and calls `mark_zones_changed` if the write failed.
    pub fn take_zones_json(&mut self) -> Option<String> {
        if !self.zones_changed {
            return None;
        }
        let json = self.zones.to_json().ok()?;
        self.zones_changed = false;
        Some(json)
    }

    /// The same for the equipments and their bindings.
    pub fn take_equipments_json(&mut self) -> Option<String> {
        if !self.equipments_changed {
            return None;
        }
        let json = self.equipments.to_json().ok()?;
        self.equipments_changed = false;
        Some(json)
    }

    /// Like `take_zones_json`, as a cursor that writes the file chunk by chunk.
    pub fn take_zones_cursor(&mut self) -> Option<ListCursor> {
        if !self.zones_changed {
            return None;
        }
        self.zones_changed = false;
        Some(ListCursor::saved_zones(self))
    }

    /// The same for the equipments.
    pub fn take_equipments_cursor(&mut self) -> Option<ListCursor> {
        if !self.equipments_changed {
            return None;
        }
        self.equipments_changed = false;
        Some(ListCursor::saved_equipments(self))
    }

    pub fn mark_zones_changed(&mut self) {
        self.zones_changed = true;
    }

    pub fn mark_equipments_changed(&mut self) {
        self.equipments_changed = true;
    }

    /// (zones changed, equipments changed) since they were last taken.
    pub fn changes(&self) -> (bool, bool) {
        (self.zones_changed, self.equipments_changed)
    }

    // ---- Zone aggregation ----

    /// The aggregated data of every zone as of `now`, with `motionSince`: the time of the first
    /// computation that saw `motion` change (after a restart, the first computation's time),
    /// unchanged while `motion` stays the same, `null` without motion sensors. Kept per zone, at
    /// most 32.
    pub fn aggregate(&mut self, now: u64) -> Vec<(Id, ZoneAggregatedData)> {
        let mut all = compute(&self.zones, &self.equipments, &self.devices, now);
        let mut next: Vec<Motion> = Vec::with_capacity(all.len());
        for (zone, data) in &mut all {
            let previous = self.motion.iter().find(|m| m.zone == *zone);
            let since = if data.motion_sensors == 0 {
                None
            } else {
                match previous {
                    Some(p) if p.since.is_some() && p.motion == data.motion => p.since,
                    _ => Some(now),
                }
            };
            data.motion_since = since;
            next.push(Motion {
                zone: *zone,
                motion: data.motion,
                since,
            });
        }
        self.motion = next;
        all
    }

    /// Brings the motion state up to date: called after each batch of measures, after the
    /// statuses were refreshed, and after each change made through the API.
    pub fn recompute(&mut self, now: u64) {
        let _ = self.aggregate(now);
    }

    // ---- Zones ----

    pub fn create_zone(&mut self, input: NewZone, now: u64, random: Random) -> Outcome<Zone> {
        let id = new_id(random)?;
        let zone = self.zones.create(input, now, id)?;
        self.zones_changed = true;
        Ok(zone)
    }

    pub fn update_zone(&mut self, id: &str, input: ZoneUpdate, now: u64) -> Outcome<Zone> {
        let zone = self.zones.update(id, input, now)?;
        self.zones_changed = true;
        Ok(zone)
    }

    /// Deletes a zone nothing is in but devices, which go to none.
    pub fn delete_zone(&mut self, id: &str, now: u64) -> Outcome<()> {
        let equipments = &self.equipments;
        self.zones
            .delete(id, |zone| equipments.count_in_zone(zone))?;
        self.zones_changed = true;
        if let Some(id) = Id::parse(id) {
            self.devices.clear_zone(&id.to_string(), now);
        }
        Ok(())
    }

    // ---- Devices ----

    /// Puts a device in a zone, or in none (`None`).
    pub fn set_device_zone(&mut self, id: &str, zone: Option<&str>, now: u64) -> Outcome<Device> {
        if self.devices.get(id).is_none() {
            return Err(Failure::new(404, "Device not found"));
        }
        let zone = match zone {
            None => None,
            Some(text) => Some(
                self.zones
                    .find(text)
                    .map(|z| z.id)
                    .ok_or_else(|| Failure::new(404, "Zone not found"))?,
            ),
        };
        self.devices
            .set_zone(id, zone, now)
            .ok_or_else(|| Failure::new(404, "Device not found"))
    }

    /// Forgets a device and the bindings made on its data and orders (the equipments stay).
    /// False when there is no such device.
    pub fn remove_device(&mut self, id: &str) -> bool {
        let Some(item) = self.devices.get(id) else {
            return false;
        };
        let data: Vec<Id> = item.data.iter().map(|d| d.id).collect();
        let orders: Vec<Id> = item.orders.iter().map(|o| o.id).collect();
        if !self.devices.remove(id) {
            return false;
        }
        for item in self.equipments.list_mut() {
            let before = item.data_bindings.len() + item.order_bindings.len();
            item.data_bindings
                .retain(|b| !data.contains(&b.device_data_id));
            item.order_bindings
                .retain(|b| !orders.contains(&b.device_order_id));
            if before != item.data_bindings.len() + item.order_bindings.len() {
                self.equipments_changed = true;
            }
        }
        true
    }
}

/// `GET /zones/aggregation`: every zone's data keyed by the zone id.
pub struct Aggregation(pub Vec<(Id, ZoneAggregatedData)>);

impl Serialize for Aggregation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.0.iter().map(|(id, data)| (id.to_string(), data)))
    }
}
