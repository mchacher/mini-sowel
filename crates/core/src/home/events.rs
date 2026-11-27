//! The events recipes listen to (Sowel's `EngineEvent`s `zone.data.changed`,
//! `equipment.data.changed` and `device.data.updated`, `src/shared/types.ts` T:1836-1872), in a
//! compact form and only when a running recipe listens to them (spec 012, R8.1): the home keeps
//! the set of what the running instances subscribed to (`Interest`) and queues the events that
//! match, at most `MAX_EVENTS`. The radio loop hands the queue to the recipe engine after each
//! batch of measures.

use std::borrow::Cow;
use std::collections::VecDeque;

use serde_json::Value;

use crate::devices::Id;
use crate::equipments::Alias;

/// Events waiting for the engine; one past it, the oldest is dropped. A batch of measures gives
/// a few events per device, the engine takes them all right after, so the queue is never near it.
pub const MAX_EVENTS: usize = 128;
/// A text value is cut at this many bytes: a state or an action, never a payload.
pub const MAX_TEXT_BYTES: usize = 32;

/// The value of a data in an event: a boolean, a number or a short text (anything else, a list
/// or an object, is `Null`: no recipe reads one).
#[derive(Debug, Clone, PartialEq)]
pub enum EventValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(Box<str>),
}

impl EventValue {
    /// A JSON value, text cut at `MAX_TEXT_BYTES` (on a character boundary).
    pub fn from_json(value: &Value) -> EventValue {
        match value {
            Value::Bool(b) => EventValue::Bool(*b),
            Value::Number(n) => n.as_f64().map_or(EventValue::Null, EventValue::Number),
            Value::String(text) => EventValue::Text(cut(text, MAX_TEXT_BYTES).into()),
            _ => EventValue::Null,
        }
    }
}

/// The text cut to at most `max` bytes, on a character boundary.
pub(crate) fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// One event, as the engine gets it.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// `zone.data.changed`: the zone's aggregate changed; what recipes read of it.
    Zone {
        zone: Id,
        motion: bool,
        luminosity: Option<i64>,
    },
    /// `equipment.data.changed`: a data bound under `alias` was updated (changed or not).
    Equipment {
        equipment: Id,
        alias: Alias,
        value: EventValue,
    },
    /// `device.data.updated`: a device data was updated (changed or not).
    Device {
        device: Id,
        key: Cow<'static, str>,
        value: EventValue,
    },
}

impl Event {
    /// The type Sowel gives it.
    pub fn name(&self) -> &'static str {
        match self {
            Event::Zone { .. } => "zone.data.changed",
            Event::Equipment { .. } => "equipment.data.changed",
            Event::Device { .. } => "device.data.updated",
        }
    }
}

/// What the running instances listen to. Bounded by what exists: a zone per instance, at most 32
/// equipments (16 lights, 16 buttons: the slots' own bound) per instance, and the motion sources,
/// which are device data (at most 64 devices of 8 data, each pair once).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Interest {
    zones: Vec<Id>,
    equipments: Vec<(Id, Alias)>,
    /// The keys are shared like aliases: the same few words (`occupancy`, `state`...) come back
    /// in every device.
    devices: Vec<(Id, Alias)>,
}

impl Interest {
    pub fn new() -> Self {
        Interest::default()
    }

    pub fn add_zone(&mut self, zone: Id) {
        if !self.zones.contains(&zone) {
            self.zones.push(zone);
        }
    }

    pub fn add_equipment(&mut self, equipment: Id, alias: Alias) {
        if !self
            .equipments
            .iter()
            .any(|(e, a)| *e == equipment && *a == alias)
        {
            self.equipments.push((equipment, alias));
        }
    }

    pub fn add_device(&mut self, device: Id, key: &str) {
        if !self
            .devices
            .iter()
            .any(|(d, k)| *d == device && &**k == key)
        {
            self.devices.push((device, Alias::new(key)));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.zones.is_empty() && self.equipments.is_empty() && self.devices.is_empty()
    }

    pub fn wants_zone(&self, zone: &Id) -> bool {
        self.zones.contains(zone)
    }

    pub fn wants_equipment(&self, equipment: &Id, alias: &str) -> bool {
        self.equipments
            .iter()
            .any(|(e, a)| e == equipment && **a == *alias)
    }

    pub fn wants_device(&self, device: &Id, key: &str) -> bool {
        self.devices.iter().any(|(d, k)| d == device && &**k == key)
    }

    /// True when some instance listens to equipments or devices (so a measure is worth looking
    /// into).
    pub fn wants_data(&self) -> bool {
        !self.equipments.is_empty() || !self.devices.is_empty()
    }
}

/// The queue of events waiting for the engine, at most `MAX_EVENTS`.
#[derive(Debug, Default)]
pub struct EventQueue {
    events: VecDeque<Event>,
}

impl EventQueue {
    /// Queues an event; past `MAX_EVENTS` the oldest is dropped, and the log says which.
    pub fn push(&mut self, event: Event) {
        if self.events.len() >= MAX_EVENTS
            && let Some(dropped) = self.events.pop_front()
        {
            log::warn!(
                "recipe events: queue full ({MAX_EVENTS}), the oldest ({}) dropped",
                dropped.name()
            );
        }
        self.events.push_back(event);
    }

    /// Every event waiting, oldest first; the queue is left empty.
    pub fn take(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events).into()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }
}
