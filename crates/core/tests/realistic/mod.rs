//! The realistic home of decision D20, built once for the tests that need it: 50 devices with 6
//! data and 1 order each, 10 zones, 50 equipments of 6 bindings (16 of them lights of the first
//! zone), and 3 instances of `motion-light`. Devices 10, 20 and 30 are the motion sensors of the
//! first zone (occupancy instead of pressure: still 6 data). `memory_ceiling.rs` measures it, `fixtures.rs`
//! writes it to `firmware/fixtures/`.
#![allow(dead_code)]

use solite_core::api::{Reach, Request, handle};
use solite_core::devices::Devices;
use solite_core::home::Home;

pub const T0: u64 = 1_795_252_323_120;

/// The motion sensors of the first zone, which the `motion-light` instances listen to.
pub fn is_motion(i: u8) -> bool {
    matches!(i, 10 | 20 | 30)
}

pub fn request<'a>(method: &'a str, path: &'a str, body: &'a [u8]) -> Request<'a> {
    Request {
        method,
        path,
        host: Some("127.0.0.1:8080"),
        port: 8080,
        reach: Reach::Loopback,
        content_type: Some("application/json"),
        authorization: None,
        body,
    }
}

/// The home and the ids the callers need.
pub struct Realistic {
    pub home: Home,
    pub zone_ids: Vec<String>,
    pub lights: Vec<String>,
    pub button: String,
    pub instance_ids: Vec<String>,
}

/// Distinct ids: a counter in the first bytes, the rest filled. The same on every call.
pub fn random() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut calls = 0u32;
    move |buffer: &mut [u8]| -> Result<(), String> {
        calls += 1;
        buffer.fill(0x5A);
        buffer[..4].copy_from_slice(&calls.to_be_bytes());
        Ok(())
    }
}

pub fn build(random: &mut impl FnMut(&mut [u8]) -> Result<(), String>) -> Realistic {
    let mut devices = Devices::new();
    for i in 0..50u8 {
        let eui = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, i];
        devices
            .identified(eui, Some("Manufacturer"), Some("Model"), T0, random)
            .unwrap();
        // Temperature, humidity, pressure (occupancy for a motion sensor), battery and voltage (5),
        // and the on/off state (1) with its order: 6 data and 1 order.
        let third = if is_motion(i) { 0x0406 } else { 0x0403 };
        devices
            .reporting_configured(eui, &[0x0402, 0x0405, third, 0x0001, 0x0006], T0, random)
            .unwrap();
        assert_eq!(devices.list()[usize::from(i)].data.len(), 6);
        assert_eq!(devices.list()[usize::from(i)].orders.len(), 1);
    }
    let device_ids: Vec<String> = devices
        .list()
        .iter()
        .map(|d| d.device.id.to_string())
        .collect();
    let mut home = Home::from_devices(devices, T0);
    let mut zone_ids = Vec::new();
    for i in 0..10 {
        let body = format!(r#"{{"name":"Zone number {i}"}}"#);
        let created = handle(
            &mut home,
            &request("POST", "/api/v1/zones", body.as_bytes()),
            T0,
            random,
        );
        assert_eq!(created.status, 201);
        let zone: serde_json::Value = serde_json::from_str(&created.body.unwrap()).unwrap();
        zone_ids.push(zone["id"].as_str().unwrap().to_string());
    }
    for (i, device) in device_ids.iter().enumerate() {
        // The last 16 are lights of the first zone, for the recipes below.
        let (kind, zone) = if i >= 34 {
            ("light_onoff", &zone_ids[0])
        } else {
            ("sensor", &zone_ids[i % 10])
        };
        let body = format!(
            r#"{{"name":"Equipment number {i}","type":"{kind}","zoneId":"{zone}","deviceIds":["{device}"]}}"#
        );
        let created = handle(
            &mut home,
            &request("POST", "/api/v1/equipments", body.as_bytes()),
            T0,
            random,
        );
        assert_eq!(created.status, 201, "{created:?}");
    }
    // 6 data bindings and 1 order binding each.
    assert_eq!(home.equipments().list().len(), 50);
    assert!(
        home.equipments()
            .list()
            .iter()
            .all(|e| e.data_bindings.len() == 6 && e.order_bindings.len() == 1)
    );

    // Spec 012: 3 instances of `motion-light`, each on the same 16 lights and the first zone,
    // with the first equipment as the button (its voltage rebound as `action`).
    let lights: Vec<String> = home
        .equipments()
        .list()
        .iter()
        .filter(|e| e.equipment.kind.is_light())
        .map(|e| e.equipment.id.to_string())
        .collect();
    assert_eq!(lights.len(), 16);
    let button = home.equipments().list()[0].equipment.id.to_string();
    let (voltage_binding, voltage_data) = {
        let item = home.equipments().get(&button).unwrap();
        let binding = item
            .data_bindings
            .iter()
            .find(|b| b.alias == "voltage")
            .unwrap();
        (binding.id.to_string(), binding.device_data_id.to_string())
    };
    let path = format!("/api/v1/equipments/{button}/data-bindings/{voltage_binding}");
    assert_eq!(
        handle(&mut home, &request("DELETE", &path, b""), T0, random).status,
        204
    );
    let path = format!("/api/v1/equipments/{button}/data-bindings");
    let body = format!(r#"{{"deviceDataId":"{voltage_data}","alias":"action"}}"#);
    assert_eq!(
        handle(
            &mut home,
            &request("POST", &path, body.as_bytes()),
            T0,
            random
        )
        .status,
        201
    );
    let mut instance_ids = Vec::new();
    for _ in 0..3 {
        let body = serde_json::json!({
            "recipeId": "motion-light",
            "params": {"zone": zone_ids[0], "lights": lights, "buttons": [button],
                       "timeout": "20s", "maxOnDuration": "5m"}
        })
        .to_string();
        let created = handle(
            &mut home,
            &request("POST", "/api/v1/recipe-instances", body.as_bytes()),
            T0,
            random,
        );
        assert_eq!(created.status, 201, "{created:?}");
        let instance: serde_json::Value = serde_json::from_str(&created.body.unwrap()).unwrap();
        instance_ids.push(instance["id"].as_str().unwrap().to_string());
    }
    Realistic {
        home,
        zone_ids,
        lights,
        button,
        instance_ids,
    }
}
