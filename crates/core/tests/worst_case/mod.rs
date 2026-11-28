//! What the tests of the worst case share: a request to the API, the devices at their largest,
//! the id a creation answers.
#![allow(dead_code)]

use std::borrow::Cow;

use serde_json::{Value, json};
use solite_core::api::{Reach, Request};
use solite_core::devices::{
    Device, DeviceData, DeviceOrder, DeviceStatus, DeviceWithDetails, Id, MAX_DATA, MAX_ORDERS,
    PowerSource, Route, ValueType,
};

pub const T0: u64 = 1_795_252_323_120;

pub fn request<'a>(method: &'a str, path: &'a str, body: &'a [u8]) -> Request<'a> {
    Request {
        method,
        path,
        host: Some("127.0.0.1:8080"),
        port: 8080,
        reach: Reach::Loopback,
        content_type: Some("application/json"),
        authorization: None,
        content_length: None,
        body,
    }
}

/// A key of exactly 32 characters, unique per device and rank.
pub fn key(prefix: &str, device: usize, rank: usize) -> String {
    format!("{:_<32}", format!("{prefix}{device:02}_{rank:02}"))
}

/// A UUID that no other call gives, for the devices built by hand.
pub fn id(counter: &mut u32) -> Id {
    *counter += 1;
    let mut bytes = [0x77u8; 16];
    bytes[..4].copy_from_slice(&counter.to_be_bytes());
    Id::from_random(bytes)
}

/// A device with `MAX_DATA` data (the last one of a light a four-field colour) and `MAX_ORDERS` orders. Devices 0 to 31 have `voltage` first
/// (the button's data and the live events'), devices 0 to 15 `state` as first order (the
/// lights'); the other keys are unique, so aliases never collide between the devices an
/// equipment binds.
pub fn device(index: usize, counter: &mut u32) -> DeviceWithDetails {
    let data = (0..MAX_DATA)
        .map(|rank| {
            let name = if rank == 0 && index < 32 {
                Cow::Borrowed("voltage")
            } else {
                Cow::Owned(key("data", index, rank))
            };
            // The last data of each light (devices 0 to 15, those with a `state` order) is a
            // colour: Zigbee2MQTT's object with its four fields, the largest value the radio can
            // give a data (spec 031). A colour object costs 632 bytes (one B-tree node); only a
            // light has a Color Control cluster, so only the 16 lights hold one.
            let colour = rank == MAX_DATA - 1 && index < 16;
            DeviceData {
                id: id(counter),
                key: name,
                value_type: if colour {
                    ValueType::Json
                } else {
                    ValueType::Number
                },
                category: Cow::Borrowed(if colour { "light_color" } else { "measure" }),
                value: if colour {
                    json!({"x": 0.3127, "y": 0.329, "hue": 359, "saturation": 100})
                } else {
                    json!(3000)
                },
                unit: (!colour).then_some(Cow::Borrowed("mV")),
                last_updated: Some(T0),
                last_changed: Some(T0),
            }
        })
        .collect();
    let orders = (0..MAX_ORDERS)
        .map(|rank| {
            let name = if rank == 0 && index < 16 {
                Cow::Borrowed("state")
            } else {
                Cow::Owned(key("order", index, rank))
            };
            DeviceOrder {
                id: id(counter),
                key: name,
                value_type: ValueType::Boolean,
                category: Some(Cow::Borrowed("light")),
                min: None,
                max: None,
                value_on: Some(Cow::Borrowed("ON")),
                value_off: Some(Cow::Borrowed("OFF")),
            }
        })
        .collect();
    DeviceWithDetails {
        device: Device {
            id: id(counter),
            source_device_id: format!("0x00124b00123456{index:02x}").into_boxed_str(),
            name: format!("{:D<64}", format!("Device {index:02}")).into_boxed_str(),
            manufacturer: Some("Manufacturer".into()),
            model: Some("Model".into()),
            zone_id: None,
            status: DeviceStatus::Online,
            power_source: PowerSource::Mains,
            last_seen: Some(T0),
            created_at: T0,
            updated_at: T0,
        },
        data,
        orders,
        route: Route {
            nwk: Some(0x1234),
            endpoint: Some(1),
        },
    }
}

pub fn created_id(reply: &solite_core::api::Response) -> String {
    let value: Value = serde_json::from_str(reply.body.as_ref().unwrap()).unwrap();
    value["id"].as_str().unwrap().to_string()
}
