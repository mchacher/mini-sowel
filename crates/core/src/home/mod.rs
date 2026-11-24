//! The home of Solite: its devices, zones and equipments together, behind the one lock the
//! host takes. The rules that touch more than one of them live here (a zone deleted frees its
//! devices, a device deleted frees the bindings made on it); each file has its change flag, so the
//! radio loop, the only writer, knows what to write. No I/O, no clock and no random source of its
//! own: the caller gives them.

mod cursor;
mod equipment_ops;
pub mod events;
pub mod pairing;
mod recipe_ops;
mod settings_ops;

pub use cursor::{CHUNK_ITEMS, ListCursor};
pub use pairing::{CLOSE_GRACE_MS, PairingWindow, Phase};
pub use settings_ops::ImportSummary;

use std::cell::{Cell, RefCell};
use std::io::{self, Write};

use serde::{Serialize, Serializer};

use crate::devices::{Device, Devices, Id, Random, format_iso};
use crate::equipments::{Alias, Equipments, StatusTracker, StoredEquipment};
use crate::failure::{Failure, Outcome};
use crate::live::{EngineEvent, LiveHandle, LiveHub, Topic};
use crate::orders::OrderQueue;
use crate::radio::MeasureEvent;
use crate::recipes::{RecipeEngine, Settle};
use crate::settings::Settings;
use crate::settings::sun::Sunlight;
use crate::zones::aggregate::{ZoneAggregatedData, compute};
use crate::zones::{NewZone, Zone, ZoneUpdate, Zones};
use events::{Event, EventQueue, EventValue, Interest};

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

/// A wall-clock time before 2020-01-01 is a clock that was never set (the box starts at 1970).
pub const CLOCK_SET_AFTER_MS: u64 = 1_577_836_800_000;

/// What the home remembers of the last aggregate it computed for a zone: its motion and
/// luminosity (what recipes read), `motionSince`, and a fingerprint of the rest to tell whether
/// the next aggregate differs (Sowel's `aggregatedDataEqual`, `motionSince` left out).
#[derive(Debug, Clone, Copy)]
struct Motion {
    zone: Id,
    motion: bool,
    luminosity: Option<i64>,
    since: Option<u64>,
    fingerprint: u64,
}

/// A 64-bit FNV-1a hash over what is written to it.
struct Fnv(u64);

impl Write for Fnv {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The fingerprint of an aggregate: its JSON, hashed. `motionSince` must still be empty.
fn fingerprint(data: &ZoneAggregatedData) -> u64 {
    let mut hash = Fnv(0xcbf2_9ce4_8422_2325);
    // Writing to a hash cannot fail; a failure would only make two aggregates look different.
    let _ = serde_json::to_writer(&mut hash, data);
    hash.0
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
    /// The orders waiting for the radio (spec 011): at most `MAX_ORDERS` of the API, and
    /// `MAX_UNATTENDED` of the core's own.
    orders: OrderQueue,
    /// The recipe instances (spec 012), with their logs.
    recipes: RecipeEngine,
    recipes_changed: bool,
    /// What the running instances listen to, and the events that match, waiting for the engine
    /// (at most `MAX_EVENTS`).
    interest: Interest,
    events: EventQueue,
    /// The clients of the WebSocket (spec 013): events are built for them only when one listens.
    live: LiveHandle,
    /// The last status of each equipment, for `equipment.status.changed`.
    tracker: StatusTracker,
    /// When the zones and statuses were last recomputed, for the periodic check (`tick`).
    recomputed_at: u64,
    /// The pairing window (spec 017): what the API asked, what the radio loop did.
    pairing: PairingWindow,
    /// The home's settings (spec 026): six values at most.
    settings: Settings,
    settings_changed: bool,
    /// The sun data and the wall-clock minute it was computed for: one value, cleared by any
    /// settings change (spec 026 R2.3).
    sun_cache: RefCell<Option<(u64, Sunlight)>>,
    /// How many times the sun data was computed (a count that stops at its maximum).
    sun_computations: Cell<u32>,
    /// The last sun data the aggregation saw, for `sunlight.changed`: one value.
    sun_seen: Sunlight,
}

/// The statuses are looked at again after this long with no recompute (a reading goes stale
/// with time alone), in milliseconds: Sowel's tracker ticks every minute.
pub const STATUS_CHECK_MS: u64 = 60_000;

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
        let live = LiveHub::handle();
        devices.set_live(LiveHandle::clone(&live));
        let orders = OrderQueue::new();
        orders.set_live(LiveHandle::clone(&live));
        let mut recipes = RecipeEngine::new();
        recipes.set_live(LiveHandle::clone(&live));
        Home {
            devices,
            zones,
            equipments,
            motion: Vec::new(),
            zones_changed: zones_repaired,
            equipments_changed: repaired,
            orders,
            recipes,
            recipes_changed: false,
            interest: Interest::new(),
            events: EventQueue::default(),
            live,
            tracker: StatusTracker::new(),
            recomputed_at: now,
            pairing: PairingWindow::new(),
            settings: Settings::default(),
            settings_changed: false,
            sun_cache: RefCell::new(None),
            sun_computations: Cell::new(0),
            sun_seen: Sunlight::default(),
        }
    }

    /// The pairing window.
    pub fn pairing(&self) -> &PairingWindow {
        &self.pairing
    }

    pub fn pairing_mut(&mut self) -> &mut PairingWindow {
        &mut self.pairing
    }

    /// The hub the WebSocket's clients are connected to.
    pub fn live(&self) -> LiveHandle {
        LiveHandle::clone(&self.live)
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

    /// The aggregated data of every zone as of `now` (the root zone with the sun data), with `motionSince`: the time of the first
    /// computation that saw `motion` change (after a restart, the first computation's time),
    /// unchanged while `motion` stays the same, `null` without motion sensors. Kept per zone, at
    /// most 32. A zone whose aggregate differs from the last one computed (every zone the first
    /// time) gives a `zone.data.changed` event when a running recipe listens to it.
    pub fn aggregate(&mut self, now: u64) -> Vec<(Id, ZoneAggregatedData)> {
        let mut all = compute(&self.zones, &self.equipments, &self.devices, now);
        // The sun data first: `sunlight.changed` comes before the root zone's `zone.data.changed`.
        self.note_sunlight(now);
        for (zone, data) in &mut all {
            self.inject_sunlight(zone, data, now);
        }
        let mut next: Vec<Motion> = Vec::with_capacity(all.len());
        for (zone, data) in &mut all {
            let previous = self.motion.iter().find(|m| m.zone == *zone);
            let print = fingerprint(data);
            let changed = previous.is_none_or(|p| p.fingerprint != print);
            if changed && self.interest.wants_zone(zone) {
                self.events.push(Event::Zone {
                    zone: *zone,
                    motion: data.motion,
                    luminosity: data.luminosity,
                });
            }
            // No time before the clock is set (a box that has not reached the time server yet counts
            // from 1970): the first computation after it gives the time.
            let since = if data.motion_sensors == 0 || now < CLOCK_SET_AFTER_MS {
                None
            } else {
                match previous {
                    Some(p) if p.since.is_some() && p.motion == data.motion => p.since,
                    _ => Some(now),
                }
            };
            data.motion_since = since;
            if changed {
                self.live
                    .emit(Topic::Zones, || EngineEvent::ZoneDataChanged {
                        zone_id: *zone,
                        aggregated_data: Box::new(data.clone()),
                    });
            }
            next.push(Motion {
                zone: *zone,
                motion: data.motion,
                luminosity: data.luminosity,
                since,
                fingerprint: print,
            });
        }
        self.motion = next;
        all
    }

    /// Brings the motion state up to date: called after each batch of measures, after the
    /// statuses were refreshed, and after each change made through the API.
    pub fn recompute(&mut self, now: u64) {
        let _ = self.aggregate(now);
        self.check_statuses(now);
        self.recomputed_at = now;
    }

    /// The periodic check: recomputes when `STATUS_CHECK_MS` passed since the last recompute, so
    /// that what goes stale with time alone is seen. The radio loop calls it on each turn.
    pub fn tick(&mut self, now: u64) {
        if now.saturating_sub(self.recomputed_at) >= STATUS_CHECK_MS {
            self.recompute(now);
        }
    }

    /// Compares the equipments' statuses with the last seen: each change is an
    /// `equipment.status.changed` event (an equipment seen for the first time is only noted).
    fn check_statuses(&mut self, now: u64) {
        let changes = self.tracker.check(&self.devices, &self.equipments, now);
        for change in changes {
            self.live
                .emit(Topic::Equipments, || EngineEvent::EquipmentStatusChanged {
                    equipment_id: change.equipment,
                    equipment_name: change.name,
                    old_status: change.old,
                    new_status: change.new,
                });
        }
    }

    /// The aggregate of one zone computed now, without remembering it.
    pub fn zone_aggregate(&self, zone: &Id, now: u64) -> Option<ZoneAggregatedData> {
        let mut data = compute(&self.zones, &self.equipments, &self.devices, now)
            .into_iter()
            .find(|(id, _)| id == zone)
            .map(|(_, data)| data)?;
        self.inject_sunlight(zone, &mut data, now);
        Some(data)
    }

    /// `(motion, luminosity)` of a zone as of the last aggregate (computed now when there is none
    /// yet); `None` for a zone that does not exist.
    pub fn zone_reading(&self, zone: &Id, now: u64) -> Option<(bool, Option<i64>)> {
        match self.motion.iter().find(|m| m.zone == *zone) {
            Some(m) => Some((m.motion, m.luminosity)),
            None => self
                .zone_aggregate(zone, now)
                .map(|d| (d.motion, d.luminosity)),
        }
    }

    /// The measures of a message, applied to the devices; each data that got a value, changed or
    /// not, gives the `device.data.updated` and `equipment.data.changed` events a running recipe
    /// listens to.
    pub fn measures(
        &mut self,
        event: &MeasureEvent,
        now: u64,
        random: Random,
    ) -> Result<(), String> {
        let Home {
            devices,
            equipments,
            interest,
            events,
            live,
            ..
        } = self;
        devices.measures_with(event, now, random, &mut |device, data, previous| {
            // The events of the clients (spec 013): built only when one listens.
            live.emit(Topic::Devices, || EngineEvent::DeviceDataUpdated {
                device_id: device.id,
                device_name: device.name.to_string(),
                data_id: data.id,
                key: data.key.to_string(),
                value: data.value.clone(),
                previous: previous.clone(),
                timestamp: format_iso(now),
            });
            if live.wants(Topic::Equipments) {
                for item in equipments.list() {
                    for binding in &item.data_bindings {
                        if binding.device_data_id == data.id {
                            live.emit(Topic::Equipments, || EngineEvent::EquipmentDataChanged {
                                equipment_id: item.equipment.id,
                                alias: binding.alias.to_string(),
                                value: data.value.clone(),
                                previous: previous.clone(),
                            });
                        }
                    }
                }
            }
            // The events of the running recipes.
            if !interest.wants_data() {
                return;
            }
            let value = EventValue::from_json(&data.value);
            if interest.wants_device(&device.id, &data.key) {
                events.push(Event::Device {
                    device: device.id,
                    key: data.key.clone(),
                    value: value.clone(),
                });
            }
            for item in equipments.list() {
                for binding in &item.data_bindings {
                    if binding.device_data_id == data.id
                        && interest.wants_equipment(&item.equipment.id, &binding.alias)
                    {
                        events.push(Event::Equipment {
                            equipment: item.equipment.id,
                            alias: binding.alias.clone(),
                            value: value.clone(),
                        });
                    }
                }
            }
        })
    }

    // ---- Zones ----

    pub fn create_zone(&mut self, input: NewZone, now: u64, random: Random) -> Outcome<Zone> {
        let id = new_id(random)?;
        let zone = self.zones.create(input, now, id)?;
        self.zones_changed = true;
        self.live.emit(Topic::Zones, || EngineEvent::ZoneCreated {
            zone: zone.clone(),
        });
        Ok(zone)
    }

    pub fn update_zone(&mut self, id: &str, input: ZoneUpdate, now: u64) -> Outcome<Zone> {
        let zone = self.zones.update(id, input, now)?;
        self.zones_changed = true;
        self.live.emit(Topic::Zones, || EngineEvent::ZoneUpdated {
            zone: zone.clone(),
        });
        Ok(zone)
    }

    /// Deletes a zone nothing is in but devices, which go to none.
    pub fn delete_zone(&mut self, id: &str, now: u64) -> Outcome<()> {
        let equipments = &self.equipments;
        let gone = self.zones.find(id).map(|z| (z.id, z.name.clone()));
        let gone_zone = gone.as_ref().map(|(zone, _)| *zone);
        self.zones
            .delete(id, |zone| equipments.count_in_zone(zone))?;
        self.zones_changed = true;
        if let Some((zone_id, zone_name)) = gone {
            self.live.emit(Topic::Zones, || EngineEvent::ZoneRemoved {
                zone_id,
                zone_name,
            });
        }
        if let Some(id) = Id::parse(id) {
            self.devices.clear_zone(&id.to_string(), now);
        }
        if let Some(zone) = gone_zone {
            self.settle_recipes(
                Settle {
                    equipment: None,
                    zone: Some(zone),
                },
                now,
            );
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

    /// Forgets a device and the bindings made on its data and orders (the equipments stay, each
    /// that lost a binding with its `equipment.updated`), then settles the recipes. False when
    /// there is no such device.
    pub fn remove_device(&mut self, id: &str, now: u64) -> bool {
        let Some(item) = self.devices.get(id) else {
            return false;
        };
        let data: Vec<Id> = item.data.iter().map(|d| d.id).collect();
        let orders: Vec<Id> = item.orders.iter().map(|o| o.id).collect();
        let gone = (item.device.id, item.device.name.to_string());
        if !self.devices.remove(id) {
            return false;
        }
        self.live
            .emit(Topic::Devices, || EngineEvent::DeviceRemoved {
                device_id: gone.0,
                device_name: gone.1,
            });
        let mut touched = Vec::new();
        for item in self.equipments.list_mut() {
            let before = item.data_bindings.len() + item.order_bindings.len();
            item.data_bindings
                .retain(|b| !data.contains(&b.device_data_id));
            item.order_bindings
                .retain(|b| !orders.contains(&b.device_order_id));
            if before != item.data_bindings.len() + item.order_bindings.len() {
                self.equipments_changed = true;
                touched.push(item.equipment.id.to_string());
            }
        }
        for equipment in &touched {
            self.announce_equipment_updated(equipment);
        }
        if !touched.is_empty() {
            self.settle_recipes(Settle::default(), now);
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
