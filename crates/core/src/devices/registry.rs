//! The devices of Solite, kept up to date from what the radio reports: added or completed at
//! pairing, values and `lastSeen` on each message, status from the time. No clock and no random
//! source of its own: the caller gives both. No I/O: the caller writes the snapshots.

use std::borrow::Cow;

use serde_json::{Number, Value};

use super::catalog::{self, DataSpec, OrderSpec};
use super::model::{
    Device, DeviceData, DeviceOrder, DeviceStatus, DeviceWithDetails, Id, PowerSource, Route,
    ValueType,
};
use crate::live::{EngineEvent, LiveHandle, Topic};
use crate::radio::measures::Value as MeasureValue;
use crate::radio::{
    Activity, Enrollment, EnrollmentOutcome, MeasureEvent, PairingEvent, RouteUpdate, Sender,
};

/// Devices kept; one past it is not added.
pub const MAX_DEVICES: usize = 64;
/// Data of one device; one past it is not added.
pub const MAX_DATA: usize = 16;
/// Orders of one device; one past it is not added.
pub const MAX_ORDERS: usize = 4;
/// Announces remembered for devices not identified yet; the oldest is dropped past this.
const MAX_ANNOUNCES: usize = 16;
/// Devices refused for lack of room, remembered to say so once each; the oldest is dropped past
/// this.
const MAX_REFUSED: usize = 16;
/// A mains device silent this long is offline, in milliseconds (Zigbee2MQTT's availability).
const MAINS_TIMEOUT_MS: u64 = 10 * 60 * 1000;
/// A battery device, or one of unknown power, silent this long is offline.
const BATTERY_TIMEOUT_MS: u64 = 25 * 60 * 60 * 1000;
/// Bit 2 of the announce's capability byte: mains powered.
const CAPABILITY_MAINS: u8 = 0x04;
/// The longest device name, in characters.
pub const MAX_NAME_CHARS: usize = 64;

/// Random bytes from the caller; the error says why there are none.
pub type Random<'a> = &'a mut dyn FnMut(&mut [u8]) -> Result<(), String>;

fn eui_string(eui64: &[u8; 8]) -> String {
    let mut text = String::from("0x");
    for byte in eui64 {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// A UUID v4 from the caller's random bytes.
fn new_id(random: Random) -> Result<Id, String> {
    let mut bytes = [0u8; 16];
    random(&mut bytes)?;
    Ok(Id::from_random(bytes))
}

/// A number as JSON: a whole number without a decimal point (`100`, not `100.0`).
pub(crate) fn json_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        // Nothing is lost: the value is whole and below 1e15.
        return Value::from(value as i64);
    }
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// The devices, with what changed since the caller last wrote them.
#[derive(Debug, Default)]
pub struct Devices {
    /// At most `MAX_DEVICES`, in the order they were added.
    items: Vec<DeviceWithDetails>,
    /// A device was added, renamed, removed, or got data or orders.
    definitions: bool,
    /// A value, a `lastSeen` or a status changed.
    values: bool,
    /// (address, capability byte) of announces of devices not in `items`; at most
    /// `MAX_ANNOUNCES`.
    announces: Vec<([u8; 8], u8)>,
    /// At most `MAX_REFUSED`.
    refused: Vec<[u8; 8]>,
    /// Where `device.status_changed` goes (spec 013), when the host listens.
    live: Option<LiveHandle>,
}

impl Devices {
    pub fn new() -> Self {
        Devices::default()
    }

    /// Where the devices tell their status changes.
    pub fn set_live(&mut self, live: LiveHandle) {
        self.live = Some(live);
    }

    /// The devices read from storage, at most `MAX_DEVICES` (and `MAX_DATA` data and
    /// `MAX_ORDERS` orders each); nothing is marked as changed.
    pub fn from_list(mut list: Vec<DeviceWithDetails>) -> Self {
        list.truncate(MAX_DEVICES);
        for item in &mut list {
            item.data.truncate(MAX_DATA);
            item.orders.truncate(MAX_ORDERS);
            // Memory: what was read has no spare room.
            item.data.shrink_to_fit();
            item.orders.shrink_to_fit();
        }
        list.shrink_to_fit();
        Devices {
            items: list,
            ..Devices::default()
        }
    }

    pub fn list(&self) -> &[DeviceWithDetails] {
        &self.items
    }

    pub fn get(&self, id: &str) -> Option<&DeviceWithDetails> {
        let id = Id::parse(id)?;
        self.items.iter().find(|d| d.device.id == id)
    }

    pub fn get_by_id(&self, id: &Id) -> Option<&DeviceWithDetails> {
        self.items.iter().find(|d| d.device.id == *id)
    }

    /// The device and the data with this id.
    pub fn data_of(&self, data_id: &Id) -> Option<(&Device, &DeviceData)> {
        self.items.iter().find_map(|item| {
            item.data
                .iter()
                .find(|d| d.id == *data_id)
                .map(|d| (&item.device, d))
        })
    }

    /// The device and the order with this id.
    pub fn order_of(&self, order_id: &Id) -> Option<(&Device, &DeviceOrder)> {
        self.items.iter().find_map(|item| {
            item.orders
                .iter()
                .find(|o| o.id == *order_id)
                .map(|o| (&item.device, o))
        })
    }

    /// Puts a device in a zone (`None`: in none). The caller checked that the zone exists.
    /// `None` when there is no such device.
    pub fn set_zone(&mut self, id: &str, zone: Option<Id>, now: u64) -> Option<Device> {
        let id = Id::parse(id)?;
        let index = self.items.iter().position(|d| d.device.id == id)?;
        if self.items[index].device.zone_id != zone {
            self.items[index].device.zone_id = zone;
            self.touch(index, now);
        }
        Some(self.items[index].device.clone())
    }

    /// Every device in a zone `known` does not accept goes in none (a file edited by hand, a
    /// crash between two files). True when one changed.
    pub fn fix_zones(&mut self, known: &dyn Fn(&str) -> bool) -> bool {
        let mut changed = false;
        for item in &mut self.items {
            if item.device.zone_id.is_some_and(|z| !known(&z.to_string())) {
                item.device.zone_id = None;
                changed = true;
            }
        }
        if changed {
            self.definitions = true;
        }
        changed
    }

    /// The devices of a zone that is gone are in none.
    pub fn clear_zone(&mut self, zone: &str, now: u64) {
        for index in 0..self.items.len() {
            if self.items[index]
                .device
                .zone_id
                .is_some_and(|z| z.to_string() == zone)
            {
                self.items[index].device.zone_id = None;
                self.touch(index, now);
            }
        }
    }

    /// A copy of the devices (tests and tools; the save path does not copy).
    pub fn snapshot(&self) -> Vec<DeviceWithDetails> {
        self.items.clone()
    }

    /// The devices as the JSON list that is saved, compact, in a buffer of the exact size.
    pub fn to_json(&self) -> Result<String, String> {
        super::persist::to_json(&super::persist::Stored(&self.items))
            .ok_or_else(|| "cannot serialise the devices".into())
    }

    /// (definitions changed, values changed) since they were last cleared.
    pub fn changes(&self) -> (bool, bool) {
        (self.definitions, self.values)
    }

    pub fn clear_changes(&mut self) {
        self.definitions = false;
        self.values = false;
    }

    pub fn mark_values_changed(&mut self) {
        self.values = true;
    }

    fn index_of(&self, eui64: &[u8; 8]) -> Option<usize> {
        let text = eui_string(eui64);
        self.items
            .iter()
            .position(|d| *d.device.source_device_id == *text)
    }

    fn touch(&mut self, index: usize, now: u64) {
        self.items[index].device.updated_at = now;
        self.definitions = true;
    }

    /// The index of the device, adding it when it is new. `Ok(None)` when there is no room
    /// (`note` then gets the line to show, once per device).
    fn ensure(
        &mut self,
        eui64: [u8; 8],
        manufacturer: Option<&str>,
        model: Option<&str>,
        now: u64,
        random: Random,
        note: &mut Vec<String>,
    ) -> Result<Option<usize>, String> {
        if let Some(index) = self.index_of(&eui64) {
            return Ok(Some(index));
        }
        if self.items.len() >= MAX_DEVICES {
            if !self.refused.contains(&eui64) {
                if self.refused.len() >= MAX_REFUSED {
                    self.refused.remove(0);
                }
                self.refused.push(eui64);
                note.push(format!(
                    "pairing: device store full ({MAX_DEVICES}), {} ignored",
                    eui_string(&eui64)
                ));
            }
            return Ok(None);
        }
        let text = eui_string(&eui64);
        let last4 = &text[text.len() - 4..];
        let name = match model {
            Some(model) => format!("{model} {last4}"),
            None => format!("Zigbee {last4}"),
        };
        let power_source = match self.announces.iter().position(|(e, _)| *e == eui64) {
            Some(i) => power_of(self.announces.remove(i).1),
            None => PowerSource::Unknown,
        };
        let device = Device {
            id: new_id(random)?,
            source_device_id: text.into_boxed_str(),
            name: name.into_boxed_str(),
            manufacturer: manufacturer.map(Box::from),
            model: model.map(Box::from),
            zone_id: None,
            status: DeviceStatus::Unknown,
            power_source,
            last_seen: Some(now),
            created_at: now,
            updated_at: now,
        };
        self.items.push(DeviceWithDetails {
            device,
            data: Vec::new(),
            orders: Vec::new(),
            route: Route::default(),
        });
        self.definitions = true;
        self.refused.retain(|e| *e != eui64);
        let index = self.items.len() - 1;
        // Sowel's `device.discovered`, on first discovery only; built when a client listens.
        if let Some(live) = &self.live {
            let device = &self.items[index].device;
            live.emit(Topic::Devices, || EngineEvent::DeviceDiscovered {
                device: device.clone(),
            });
        }
        Ok(Some(index))
    }

    /// A device was identified: added if new (named `<model> <last 4 digits>`), else its
    /// manufacturer and model are brought up to date. Returns the lines to show.
    pub fn identified(
        &mut self,
        eui64: [u8; 8],
        manufacturer: Option<&str>,
        model: Option<&str>,
        now: u64,
        random: Random,
    ) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();
        let existed = self.index_of(&eui64).is_some();
        let Some(index) = self.ensure(eui64, manufacturer, model, now, random, &mut notes)? else {
            return Ok(notes);
        };
        if existed {
            let device = &mut self.items[index].device;
            let mut changed = false;
            if manufacturer.is_some() && device.manufacturer.as_deref() != manufacturer {
                device.manufacturer = manufacturer.map(Box::from);
                changed = true;
            }
            if model.is_some() && device.model.as_deref() != model {
                device.model = model.map(Box::from);
                changed = true;
            }
            if changed {
                self.touch(index, now);
            }
        }
        self.mark_seen(index, now);
        self.refresh_status(now);
        Ok(notes)
    }

    /// The reporting of these clusters was set up: the device gets their data and orders (the
    /// ones it has are kept, none is duplicated). Returns the lines to show.
    pub fn reporting_configured(
        &mut self,
        eui64: [u8; 8],
        clusters: &[u16],
        now: u64,
        random: Random,
    ) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();
        let Some(index) = self.ensure(eui64, None, None, now, random, &mut notes)? else {
            return Ok(notes);
        };
        let mut added = false;
        for cluster in clusters {
            for spec in catalog::data_for_cluster(*cluster) {
                added |= self.add_data(index, spec, random)?;
            }
            for spec in catalog::orders_for_cluster(*cluster) {
                added |= self.add_order(index, spec, random)?;
            }
        }
        if added {
            // The order of the table of spec 008, whatever the order the clusters came in.
            self.items[index]
                .data
                .sort_by_key(|d| catalog::data_rank(&d.key));
            self.items[index]
                .orders
                .sort_by_key(|o| catalog::order_rank(&o.key));
            // Memory: no spare room kept for data and orders that have a bound.
            self.items[index].data.shrink_to_fit();
            self.items[index].orders.shrink_to_fit();
            self.touch(index, now);
        }
        self.mark_seen(index, now);
        self.refresh_status(now);
        Ok(notes)
    }

    /// The enrollment of an IAS zone device ended: unless it failed, the device gets the data of
    /// its zone type (the ones it has are kept). Returns the lines to show.
    pub fn zone_enrolled(
        &mut self,
        eui64: [u8; 8],
        enrollment: &Enrollment,
        now: u64,
        random: Random,
    ) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();
        let Some(zone_type) = enrollment
            .zone_type
            .filter(|_| enrollment.outcome != EnrollmentOutcome::Failed)
        else {
            return Ok(notes);
        };
        let Some(index) = self.ensure(eui64, None, None, now, random, &mut notes)? else {
            return Ok(notes);
        };
        let mut added = false;
        for spec in catalog::data_for_zone_type(zone_type) {
            added |= self.add_data(index, spec, random)?;
        }
        if added {
            self.sorted(index, now);
        }
        Ok(notes)
    }

    /// Puts the data of a device in the order of the table of spec 008, with no spare room.
    fn sorted(&mut self, index: usize, now: u64) {
        self.items[index]
            .data
            .sort_by_key(|d| catalog::data_rank(&d.key));
        self.items[index].data.shrink_to_fit();
        self.touch(index, now);
    }

    fn add_data(&mut self, index: usize, spec: &DataSpec, random: Random) -> Result<bool, String> {
        let item = &self.items[index];
        if item.data.iter().any(|d| d.key == spec.key) {
            return Ok(false);
        }
        if item.data.len() >= MAX_DATA {
            log::warn!(
                "{}: more than {MAX_DATA} data, {} not added",
                item.device.source_device_id,
                spec.key
            );
            return Ok(false);
        }
        let data = DeviceData {
            id: new_id(random)?,
            key: Cow::Borrowed(spec.key),
            value_type: spec.value_type,
            category: Cow::Borrowed(spec.category),
            value: Value::Null,
            unit: spec.unit.map(Cow::Borrowed),
            last_updated: None,
            last_changed: None,
        };
        self.items[index].data.push(data);
        Ok(true)
    }

    fn add_order(
        &mut self,
        index: usize,
        spec: &OrderSpec,
        random: Random,
    ) -> Result<bool, String> {
        let item = &self.items[index];
        if item.orders.iter().any(|o| o.key == spec.key) {
            return Ok(false);
        }
        if item.orders.len() >= MAX_ORDERS {
            log::warn!(
                "{}: more than {MAX_ORDERS} orders, {} not added",
                item.device.source_device_id,
                spec.key
            );
            return Ok(false);
        }
        let order = DeviceOrder {
            id: new_id(random)?,
            key: Cow::Borrowed(spec.key),
            value_type: spec.value_type,
            category: Some(Cow::Borrowed(spec.category)),
            min: spec.min,
            max: spec.max,
            value_on: spec.value_on.map(Cow::Borrowed),
            value_off: spec.value_off.map(Cow::Borrowed),
        };
        self.items[index].orders.push(order);
        Ok(true)
    }

    fn mark_seen(&mut self, index: usize, now: u64) {
        self.items[index].device.last_seen = Some(now);
        self.values = true;
    }

    /// The device announced itself: its power source, from the capability byte. Remembered for
    /// a device not added yet.
    pub fn announced(&mut self, eui64: [u8; 8], capability: u8, now: u64) {
        let Some(index) = self.index_of(&eui64) else {
            self.announces.retain(|(e, _)| *e != eui64);
            if self.announces.len() >= MAX_ANNOUNCES {
                self.announces.remove(0);
            }
            self.announces.push((eui64, capability));
            return;
        };
        let power = power_of(capability);
        if self.items[index].device.power_source != power {
            self.items[index].device.power_source = power;
            self.touch(index, now);
        }
        self.mark_seen(index, now);
        self.refresh_status(now);
    }

    /// The device sent a message: `lastSeen`. A sender known by its network address only, or
    /// not in the store, is ignored.
    pub fn seen(&mut self, sender: Sender, now: u64) {
        let Sender::Eui64(eui64) = sender else {
            return;
        };
        if let Some(index) = self.index_of(&eui64) {
            self.mark_seen(index, now);
            self.refresh_status(now);
        }
    }

    /// How a device is reached, as its messages told: a change is a change of definitions (written
    /// at once; it is rare). A device not in the store is ignored.
    pub fn set_route(&mut self, update: &RouteUpdate) {
        let Some(index) = self.index_of(&update.eui64) else {
            return;
        };
        let route = &mut self.items[index].route;
        let new = Route {
            nwk: Some(update.nwk),
            endpoint: update.endpoint.or(route.endpoint),
        };
        if *route != new {
            *route = new;
            self.definitions = true;
        }
    }

    /// The device with this address, for sending it an order.
    pub fn by_eui64(&self, eui64: &[u8; 8]) -> Option<&DeviceWithDetails> {
        self.index_of(eui64).map(|i| &self.items[i])
    }

    /// The measures of a message: the matching data get their value. A measure the device has
    /// no data for, and a device not in the store, are ignored. The `alarm` of a zone sets the
    /// zone's data the device has. The first time a zone status bit is set, `tamper` or
    /// `battery_low` is added (spec 009, R4.5): every path goes through here, so none can miss it.
    pub fn measures(
        &mut self,
        event: &MeasureEvent,
        now: u64,
        random: Random,
    ) -> Result<(), String> {
        self.measures_with(event, now, random, &mut |_, _, _| {})
    }

    /// `measures`, telling `updated` each data that got a value (changed or not): its device,
    /// the data as it is now, and the value it had. The home turns these into the events
    /// recipes and clients listen to.
    pub fn measures_with(
        &mut self,
        event: &MeasureEvent,
        now: u64,
        random: Random,
        updated: &mut dyn FnMut(&Device, &DeviceData, &Value),
    ) -> Result<(), String> {
        let Sender::Eui64(eui64) = event.sender else {
            return Ok(());
        };
        let Some(index) = self.index_of(&eui64) else {
            return Ok(());
        };
        let mut added = false;
        for measure in &event.measures {
            if let (Some(spec), MeasureValue::Bool(true)) =
                (catalog::data_for_zone_flag(measure.kind), measure.value)
            {
                added |= self.add_data(index, &spec, random)?;
            }
        }
        if added {
            self.sorted(index, now);
        }
        for measure in &event.measures {
            let DeviceWithDetails { device, data, .. } = &mut self.items[index];
            if measure.kind == "alarm" {
                // The zone's data: `contact` is true when closed, the alarm bit clear.
                let data = data
                    .iter_mut()
                    .find(|d| catalog::ZONE_KEYS.contains(&&*d.key));
                if let (Some(data), MeasureValue::Bool(alarm)) = (data, measure.value) {
                    let on = if data.key == "contact" { !alarm } else { alarm };
                    let previous = set_value(data, Value::Bool(on), now);
                    updated(device, data, &previous);
                }
                continue;
            }
            let Some(key) = catalog::key_for_measure(measure.kind) else {
                continue;
            };
            let Some(data) = data.iter_mut().find(|d| d.key == key) else {
                continue;
            };
            let value = match (measure.value, data.value_type) {
                (MeasureValue::Number(n), ValueType::Number) => {
                    json_number(measure.device_value.unwrap_or(n))
                }
                (MeasureValue::Bool(on), ValueType::Boolean) => Value::Bool(on),
                _ => continue,
            };
            let previous = set_value(data, value, now);
            updated(device, data, &previous);
        }
        self.mark_seen(index, now);
        self.refresh_status(now);
        Ok(())
    }

    /// Applies one event of the radio. Returns the lines to show.
    pub fn apply(
        &mut self,
        event: &PairingEvent,
        now: u64,
        random: Random,
    ) -> Result<Vec<String>, String> {
        match event {
            PairingEvent::Identified {
                eui64,
                manufacturer,
                model,
            } => self.identified(
                *eui64,
                manufacturer.as_deref(),
                model.as_deref(),
                now,
                random,
            ),
            PairingEvent::ReportingConfigured {
                eui64,
                ok,
                enrollment,
                ..
            } => {
                let mut notes = self.reporting_configured(*eui64, ok, now, random)?;
                if let Some(enrollment) = enrollment {
                    notes.extend(self.zone_enrolled(*eui64, enrollment, now, random)?);
                }
                Ok(notes)
            }
            PairingEvent::Measures(event) => {
                self.measures(event, now, random)?;
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Applies what a device did (see `Radio::take_activity`).
    pub fn apply_activity(&mut self, activity: &Activity, now: u64) {
        match activity {
            Activity::Announced { eui64, capability } => self.announced(*eui64, *capability, now),
            Activity::Message { sender } => self.seen(*sender, now),
        }
    }

    /// Brings each status up to date with the time: online within 10 minutes of the last
    /// message for a mains device, 25 hours for a battery device or one of unknown power,
    /// offline past it, unknown before any message. A change of status changes `updatedAt`.
    /// True when a status changed.
    pub fn refresh_status(&mut self, now: u64) -> bool {
        let mut changed = false;
        for index in 0..self.items.len() {
            let device = &self.items[index].device;
            let status = match device.last_seen {
                None => DeviceStatus::Unknown,
                Some(seen) => {
                    let limit = match device.power_source {
                        PowerSource::Mains => MAINS_TIMEOUT_MS,
                        _ => BATTERY_TIMEOUT_MS,
                    };
                    if now.saturating_sub(seen) <= limit {
                        DeviceStatus::Online
                    } else {
                        DeviceStatus::Offline
                    }
                }
            };
            if status != device.status {
                self.items[index].device.status = status;
                self.items[index].device.updated_at = now;
                self.values = true;
                changed = true;
                if let Some(live) = &self.live {
                    let device = &self.items[index].device;
                    live.emit(Topic::Devices, || EngineEvent::DeviceStatusChanged {
                        device_id: device.id,
                        device_name: device.name.to_string(),
                        status,
                    });
                }
            }
        }
        changed
    }

    /// Renames a device. `None` when there is no such device.
    pub fn rename(&mut self, id: &str, name: &str, now: u64) -> Option<Device> {
        let id = Id::parse(id)?;
        let index = self.items.iter().position(|d| d.device.id == id)?;
        if *self.items[index].device.name != *name {
            self.items[index].device.name = name.into();
            self.touch(index, now);
        }
        Some(self.items[index].device.clone())
    }

    /// Forgets a device. False when there is no such device.
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(index) =
            Id::parse(id).and_then(|id| self.items.iter().position(|d| d.device.id == id))
        else {
            return false;
        };
        self.items.remove(index);
        self.definitions = true;
        true
    }
}

/// A value was received: `lastUpdated` is now, `lastChanged` only when the value differs from
/// the previous one.
fn set_value(data: &mut DeviceData, value: Value, now: u64) -> Value {
    let previous = data.value.clone();
    if data.value != value {
        data.last_changed = Some(now);
        data.value = value;
    }
    data.last_updated = Some(now);
    previous
}

fn power_of(capability: u8) -> PowerSource {
    if capability & CAPABILITY_MAINS != 0 {
        PowerSource::Mains
    } else {
        PowerSource::Battery
    }
}
