//! What the tests of zones, equipments and aggregation share: a home that answers API calls at a
//! time the test sets, devices of the garage (a SNZB-02, a motion sensor, a relay), devices made
//! from JSON for the categories no catalog device gives, and measure events.
#![allow(dead_code)]

pub mod update_fake;

use serde_json::{Value as Json, json};
use solite_core::api::{Reach, Request, Response, handle};
use solite_core::devices::{Devices, parse_list};
use solite_core::home::Home;
use solite_core::orders::DeviceCommand;
use solite_core::radio::measures::{Measure, Value};
use solite_core::radio::zcl::Command;
use solite_core::radio::{MeasureEvent, Sender};

pub const T0: u64 = 1_795_252_323_120; // 2026-11-21T09:12:03.120Z
pub const MINUTE: u64 = 60_000;
pub const HOUR: u64 = 60 * MINUTE;
pub const PORT: u16 = 8080;
pub const HOST: &str = "127.0.0.1:8080";
pub const ROOT: &str = "00000000-0000-0000-0000-000000000001";

pub fn eui(last: u8) -> [u8; 8] {
    [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, last]
}

pub type Random = Box<dyn FnMut(&mut [u8]) -> Result<(), String>>;

/// Random bytes that differ from call to call (every id is distinct).
pub fn counter() -> Random {
    let mut n = 0u8;
    let mut round = 0u8;
    Box::new(move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        if n == 0 {
            round = round.wrapping_add(1);
        }
        buffer.fill(n);
        buffer[0] = round;
        buffer[1] = n.wrapping_mul(31);
        Ok(())
    })
}

/// A home, the time, and a source of random bytes.
pub struct Rig {
    pub home: Home,
    pub now: u64,
    pub random: Random,
}

impl Rig {
    pub fn new() -> Rig {
        Rig {
            home: Home::new(T0),
            now: T0,
            random: counter(),
        }
    }

    /// A home with these devices.
    pub fn with(devices: Devices) -> Rig {
        Rig {
            home: Home::from_devices(devices, T0),
            now: T0,
            random: counter(),
        }
    }

    pub fn raw(&mut self, method: &str, path: &str, body: &str) -> Response {
        handle(
            &mut self.home,
            &Request {
                method,
                path,
                host: Some(HOST),
                port: PORT,
                reach: Reach::Loopback,
                content_type: Some("application/json"),
                authorization: None,
                body: body.as_bytes(),
            },
            self.now,
            &mut *self.random,
        )
    }

    /// (status, body as JSON; `null` without a body).
    pub fn call(&mut self, method: &str, path: &str, body: &str) -> (u16, Json) {
        let response = self.raw(method, path, body);
        let body = response
            .body
            .map_or(Json::Null, |text| serde_json::from_str(&text).unwrap());
        (response.status, body)
    }

    pub fn get(&mut self, path: &str) -> (u16, Json) {
        self.call("GET", path, "")
    }

    pub fn post(&mut self, path: &str, body: Json) -> (u16, Json) {
        self.call("POST", path, &body.to_string())
    }

    pub fn put(&mut self, path: &str, body: Json) -> (u16, Json) {
        self.call("PUT", path, &body.to_string())
    }

    pub fn delete(&mut self, path: &str) -> (u16, Json) {
        self.call("DELETE", path, "")
    }

    /// Like `call` but only the status and the error text of a refused request.
    pub fn refused(&mut self, method: &str, path: &str, body: &str) -> (u16, String) {
        let (status, body) = self.call(method, path, body);
        (status, body["error"].as_str().unwrap_or("").to_string())
    }

    /// Creates a zone; its id.
    pub fn zone(&mut self, name: &str) -> String {
        let (status, body) = self.post("/api/v1/zones", json!({ "name": name }));
        assert_eq!(status, 201, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    /// Creates an equipment from devices; its id.
    pub fn equipment(&mut self, name: &str, kind: &str, zone: &str, devices: &[&str]) -> String {
        let (status, body) = self.post(
            "/api/v1/equipments",
            json!({ "name": name, "type": kind, "zoneId": zone, "deviceIds": devices }),
        );
        assert_eq!(status, 201, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    /// The id of the device of this EUI.
    pub fn device(&self, last: u8) -> String {
        let text = format!("0x00124b00123456{last:02x}");
        self.home
            .devices
            .list()
            .iter()
            .find(|d| *d.device.source_device_id == *text)
            .unwrap()
            .device
            .id
            .to_string()
    }

    /// A SNZB-02: temperature, humidity, battery and voltage; battery powered.
    pub fn sensor(&mut self, last: u8) -> String {
        let e = eui(last);
        self.home.devices.announced(e, 0x80, self.now);
        self.home
            .devices
            .identified(
                e,
                Some("eWeLink"),
                Some("TH01"),
                self.now,
                &mut *self.random,
            )
            .unwrap();
        self.home
            .devices
            .reporting_configured(e, &[0x0402, 0x0405, 0x0001], self.now, &mut *self.random)
            .unwrap();
        self.device(last)
    }

    /// An occupancy sensor, battery powered.
    pub fn motion_sensor(&mut self, last: u8) -> String {
        let e = eui(last);
        self.home.devices.announced(e, 0x80, self.now);
        self.home
            .devices
            .identified(
                e,
                Some("eWeLink"),
                Some("MS01"),
                self.now,
                &mut *self.random,
            )
            .unwrap();
        self.home
            .devices
            .reporting_configured(e, &[0x0406], self.now, &mut *self.random)
            .unwrap();
        self.device(last)
    }

    /// A relay: the boolean `state` (a `light_state`) and its order; mains powered.
    pub fn relay(&mut self, last: u8) -> String {
        let e = eui(last);
        self.home.devices.announced(e, 0x8C, self.now);
        self.home
            .devices
            .identified(
                e,
                Some("SONOFF"),
                Some("ZBMINIR2"),
                self.now,
                &mut *self.random,
            )
            .unwrap();
        self.home
            .devices
            .reporting_configured(e, &[0x0006], self.now, &mut *self.random)
            .unwrap();
        self.device(last)
    }

    /// Applies a measure event now, then brings the aggregation up to date (the radio loop).
    pub fn measure(&mut self, event: &MeasureEvent) {
        self.home
            .devices
            .measures(event, self.now, &mut *self.random)
            .unwrap();
        self.home.recompute(self.now);
    }

    /// Adds devices written as JSON (see `device_json`).
    pub fn add_devices(&mut self, list: Vec<Json>) {
        let mut all = self.home.devices.snapshot();
        let parsed = parse_list(&Json::Array(list).to_string()).unwrap();
        all.extend(parsed);
        let (zones, equipments) = (
            self.home.zones().list().to_vec(),
            self.home.equipments().list().to_vec(),
        );
        self.home = Home::from_parts(Devices::from_list(all), zones, equipments, self.now);
    }
}

fn event(last: u8, kind: &'static str, value: Value, unit: &'static str) -> MeasureEvent {
    MeasureEvent {
        sender: Sender::Eui64(eui(last)),
        endpoint: 1,
        measures: vec![Measure {
            kind,
            value,
            unit,
            device_value: None,
        }],
    }
}

pub fn luminosity(last: u8, value: f64) -> MeasureEvent {
    event(last, "luminosity", Value::Number(value), "lx")
}

pub fn temperature(last: u8, value: f64) -> MeasureEvent {
    event(last, "temperature", Value::Number(value), "°C")
}

pub fn humidity(last: u8, value: f64) -> MeasureEvent {
    event(last, "humidity", Value::Number(value), "%")
}

pub fn motion(last: u8, on: bool) -> MeasureEvent {
    event(last, "motion", Value::Bool(on), "")
}

pub fn state(last: u8, on: bool) -> MeasureEvent {
    event(last, "light_state", Value::Bool(on), "")
}

/// A device written as JSON, mains powered unless `battery`, last seen at `T0`, with data given
/// as `(key, category, type, value, lastUpdated in ms or None)`. The id is `n` repeated.
pub fn device_json(
    n: u8,
    name: &str,
    battery: bool,
    data: &[(&str, &str, &str, Json, Option<u64>)],
) -> Json {
    let id = |prefix: u8, k: u8, j: u8| {
        format!("{prefix:02x}{k:02x}{j:02x}00-0000-4000-8000-000000000000")
    };
    let device_id = id(0xd0, n, 0);
    let items: Vec<Json> = data
        .iter()
        .enumerate()
        .map(|(i, (key, category, kind, value, updated))| {
            json!({
                "id": id(0xda, n, u8::try_from(i).unwrap()),
                "deviceId": device_id,
                "key": key,
                "type": kind,
                "category": category,
                "value": value,
                "lastUpdated": updated.map(solite_core::devices::format_iso),
                "lastChanged": updated.map(solite_core::devices::format_iso),
            })
        })
        .collect();
    json!({
        "id": device_id,
        "integrationId": "solite",
        "sourceDeviceId": format!("0x00124b00ff{n:06x}"),
        "name": name,
        "zoneId": null,
        "source": "solite",
        "status": "online",
        "powerSource": if battery { "battery" } else { "mains" },
        "lastSeen": solite_core::devices::format_iso(T0),
        "createdAt": solite_core::devices::format_iso(T0),
        "updatedAt": solite_core::devices::format_iso(T0),
        "data": items,
        "orders": [],
    })
}

/// The id of the device `device_json(n, ...)` gives.
pub fn device_json_id(device: &Json) -> String {
    device["id"].as_str().unwrap().to_string()
}

/// The id of data number `i` of a device made by `device_json`.
pub fn data_id(device: &Json, i: usize) -> String {
    device["data"][i]["id"].as_str().unwrap().to_string()
}

// ---- Recipes (spec 012) ----

/// The garage of the recipe tests: a zone, a motion and luminosity sensor in an equipment, a
/// relay in a `light_onoff` equipment.
pub struct Garage {
    pub zone: String,
    pub motion_device: String,
    pub motion_equipment: String,
    pub relay_device: String,
    pub light: String,
}

pub const PIR: u8 = 0x50;
pub const RELAY: u8 = 0x7A;

impl Rig {
    /// The garage; its relay is OFF (it reported once) and nothing moves.
    pub fn garage(&mut self) -> Garage {
        let zone = self.zone("Garage");
        let e = eui(PIR);
        self.home.devices.announced(e, 0x80, self.now);
        self.home
            .devices
            .identified(
                e,
                Some("eWeLink"),
                Some("MS01"),
                self.now,
                &mut *self.random,
            )
            .unwrap();
        self.home
            .devices
            .reporting_configured(e, &[0x0406, 0x0400], self.now, &mut *self.random)
            .unwrap();
        let motion_device = self.device(PIR);
        let relay_device = self.relay(RELAY);
        let motion_equipment = self.equipment("Garage motion", "sensor", &zone, &[&motion_device]);
        let light = self.equipment("Garage light", "light_onoff", &zone, &[&relay_device]);
        self.measure(&state(RELAY, false));
        self.measure(&motion(PIR, false));
        Garage {
            zone,
            motion_device,
            motion_equipment,
            relay_device,
            light,
        }
    }

    /// One turn of the radio loop at the current time: the zones are recomputed, the recipes get
    /// the events waiting and their timers fire.
    pub fn turn(&mut self) {
        self.home.recompute(self.now);
        self.home.run_recipes(self.now, self.now);
    }

    /// Time passes (milliseconds), with a loop turn at the end.
    pub fn advance(&mut self, ms: u64) {
        self.now += ms;
        self.turn();
    }

    /// A message of a device, as the radio loop applies it: the measures (with their events), the
    /// zones recomputed, the recipes run.
    pub fn report(&mut self, event: &MeasureEvent) {
        self.home
            .measures(event, self.now, &mut *self.random)
            .unwrap();
        self.turn();
    }

    /// The commands of the orders waiting in the queue, in the order they were queued; the queue
    /// is emptied (as the radio loop would send them).
    pub fn sent(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        while let Some(queued) = self.home.orders().take(self.now) {
            commands.extend(
                queued
                    .request
                    .commands
                    .iter()
                    .map(|c: &DeviceCommand| c.command),
            );
            self.home.orders().done();
        }
        commands
    }

    /// Creates an instance of `motion-light`; (status, body).
    pub fn instance(&mut self, params: Json) -> (u16, Json) {
        self.post(
            "/api/v1/recipe-instances",
            json!({ "recipeId": "motion-light", "params": params }),
        )
    }

    /// An instance of the garage's light with these parameters added to the zone and the light.
    pub fn garage_instance(&mut self, garage: &Garage, extra: Json) -> String {
        let mut params = json!({
            "zone": garage.zone,
            "lights": [garage.light],
            "timeout": "20s",
        });
        if let (Some(params), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                params.insert(key.clone(), value.clone());
            }
        }
        let (status, body) = self.instance(params);
        assert_eq!(status, 201, "{body}");
        body["id"].as_str().unwrap().to_string()
    }

    /// The messages of an instance's log, oldest first.
    pub fn messages(&mut self, id: &str) -> Vec<String> {
        let (status, body) = self.get(&format!("/api/v1/recipe-instances/{id}/log"));
        assert_eq!(status, 200);
        let mut list: Vec<String> = body
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["message"].as_str().unwrap().to_string())
            .collect();
        list.reverse();
        list
    }

    /// The state keys of an instance, from `GET /recipe-instances`.
    pub fn state(&mut self, id: &str) -> Json {
        let (status, body) = self.get("/api/v1/recipe-instances");
        assert_eq!(status, 200);
        body.as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == id)
            .unwrap()["state"]
            .clone()
    }
}
