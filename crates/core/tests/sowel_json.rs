//! A device serialised equals an example written by hand from Sowel's `types.ts`
//! (`Device` T:185, `DeviceData` T:225, `DeviceOrder` T:236, `DeviceWithDetails` T:259): field
//! names, camelCase, enum values, `null` and omitted fields. The example is not produced by the
//! code under test. Spec 010 adds the zone (`Zone` T:268, `ZoneWithChildren` T:279), the equipment
//! (`Equipment` T:419, `DataBinding` T:523, `OrderBinding` T:576, `DataBindingWithValue` T:583,
//! `OrderBindingWithDetails` T:613, `EquipmentWithDetails` T:663) and the aggregation
//! (`ZoneAggregatedData` T:283), written by hand from `types.ts`.

mod support;

use serde_json::json;

use solite_core::devices::Devices;
use support::Rig;

const T0: u64 = 1_795_252_323_120; // 2026-11-21T09:12:03.120Z
const EUI: [u8; 8] = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0x78];

/// Random bytes that are all `n`, `n` going 1, 2, 3...: the ids are `0101...`, `0202...`.
fn counter() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        buffer.fill(n);
        Ok(())
    }
}

#[test]
fn a_snzb02_serialises_as_sowels_device_with_details() {
    let mut devices = Devices::new();
    let mut random = counter();
    devices.announced(EUI, 0x80, T0);
    devices
        .identified(EUI, Some("eWeLink"), Some("TH01"), T0, &mut random)
        .unwrap();
    devices
        .reporting_configured(EUI, &[0x0402, 0x0405, 0x0001], T0, &mut random)
        .unwrap();
    let actual = serde_json::to_value(&devices.list()[0]).unwrap();
    // Ids: the device 1, then temperature 2, humidity 3, battery 4, voltage 5.
    let expected = json!({
        "id": "01010101-0101-4101-8101-010101010101",
        "integrationId": "solite",
        "sourceDeviceId": "0x00124b0012345678",
        "name": "TH01 5678",
        "manufacturer": "eWeLink",
        "model": "TH01",
        "ieeeAddress": "0x00124b0012345678",
        "zoneId": null,
        "source": "solite",
        "status": "online",
        "powerSource": "battery",
        "lastSeen": "2026-11-21T09:12:03.120Z",
        "createdAt": "2026-11-21T09:12:03.120Z",
        "updatedAt": "2026-11-21T09:12:03.120Z",
        "data": [
            {
                "id": "02020202-0202-4202-8202-020202020202",
                "deviceId": "01010101-0101-4101-8101-010101010101",
                "key": "temperature",
                "type": "number",
                "category": "temperature",
                "value": null,
                "unit": "°C",
                "lastUpdated": null
            },
            {
                "id": "03030303-0303-4303-8303-030303030303",
                "deviceId": "01010101-0101-4101-8101-010101010101",
                "key": "humidity",
                "type": "number",
                "category": "humidity",
                "value": null,
                "unit": "%",
                "lastUpdated": null
            },
            {
                "id": "04040404-0404-4404-8404-040404040404",
                "deviceId": "01010101-0101-4101-8101-010101010101",
                "key": "battery",
                "type": "number",
                "category": "battery",
                "value": null,
                "unit": "%",
                "lastUpdated": null
            },
            {
                "id": "05050505-0505-4505-8505-050505050505",
                "deviceId": "01010101-0101-4101-8101-010101010101",
                "key": "voltage",
                "type": "number",
                "category": "voltage",
                "value": null,
                "unit": "mV",
                "lastUpdated": null
            }
        ],
        "orders": []
    });
    assert_eq!(actual, expected);
}

#[test]
fn a_relay_has_the_boolean_state_data_without_unit_and_the_toggle_order() {
    let mut devices = Devices::new();
    let mut random = counter();
    devices.announced(EUI, 0x8C, T0);
    devices
        .identified(EUI, Some("SONOFF"), Some("ZBMINIR2"), T0, &mut random)
        .unwrap();
    devices
        .reporting_configured(EUI, &[0x0006], T0, &mut random)
        .unwrap();
    let actual = serde_json::to_value(&devices.list()[0]).unwrap();
    assert_eq!(actual["powerSource"], "mains");
    assert_eq!(
        actual["data"],
        json!([{
            "id": "02020202-0202-4202-8202-020202020202",
            "deviceId": "01010101-0101-4101-8101-010101010101",
            "key": "state",
            "type": "boolean",
            "category": "light_state",
            "value": null,
            "lastUpdated": null
        }])
    );
    assert_eq!(
        actual["orders"],
        json!([{
            "id": "03030303-0303-4303-8303-030303030303",
            "deviceId": "01010101-0101-4101-8101-010101010101",
            "key": "state",
            "type": "boolean",
            "category": "light_toggle",
            "valueOn": "ON",
            "valueOff": "OFF"
        }])
    );
}

#[test]
fn a_device_of_unknown_manufacturer_and_model_omits_them_and_is_named_zigbee() {
    let mut devices = Devices::new();
    devices
        .identified(EUI, None, None, T0, &mut counter())
        .unwrap();
    let actual = serde_json::to_value(&devices.list()[0]).unwrap();
    assert_eq!(actual["name"], "Zigbee 5678");
    assert_eq!(actual["powerSource"], "unknown");
    let object = actual.as_object().unwrap();
    assert!(!object.contains_key("manufacturer"));
    assert!(!object.contains_key("model"));
    assert!(object.contains_key("zoneId"));
}

// ---- Spec 010: zones, equipments, aggregation ----

/// The UUID v4 the random bytes `n n n ...` give (version 4, variant 10): `0a0a0a0a-0a0a-4a0a...`.
fn uuid(n: u8) -> String {
    let b = |i: usize| match i {
        6 => (n & 0x0F) | 0x40,
        8 => (n & 0x3F) | 0x80,
        _ => n,
    };
    let hex =
        |range: std::ops::Range<usize>| range.map(|i| format!("{:02x}", b(i))).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        hex(0..4),
        hex(4..6),
        hex(6..8),
        hex(8..10),
        hex(10..16)
    )
}

/// A home with a garage: ids come out as `uuid(1)`, `uuid(2)`...: the SNZB-02 is 1 (data 2 to 5),
/// the relay 6 (data 7, order 8), the zone 9, the sensor equipment 10 (bindings 11 to 14), the
/// light equipment 15 (data binding 16, order binding 17).
fn garage() -> Rig {
    let mut rig = Rig::new();
    rig.random = Box::new(|buffer: &mut [u8]| {
        thread_local!(static N: std::cell::Cell<u8> = const { std::cell::Cell::new(0) });
        let n = N.with(|n| {
            n.set(n.get().wrapping_add(1));
            n.get()
        });
        buffer.fill(n);
        Ok(())
    });
    // The counter is per thread and a test runs in its own thread: it starts at 0.
    let th = rig.sensor(0x78);
    let relay = rig.relay(0x7A);
    let zone = rig.zone("Garage");
    rig.equipment("Climate", "sensor", &zone, &[&th]);
    rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    rig
}

#[test]
fn a_zone_and_the_tree_serialise_as_sowels_zone_and_zone_with_children() {
    let mut rig = garage();
    let zone = rig.get(&format!("/api/v1/zones/{}", uuid(9))).1;
    assert_eq!(
        zone,
        json!({
            "id": uuid(9),
            "name": "Garage",
            "parentId": "00000000-0000-0000-0000-000000000001",
            "displayOrder": 0,
            "createdAt": "2026-11-21T09:12:03.120Z",
            "updatedAt": "2026-11-21T09:12:03.120Z",
            "children": []
        })
    );
    let tree = rig.get("/api/v1/zones").1;
    assert_eq!(
        tree,
        json!([{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Maison",
            "parentId": null,
            "icon": "home",
            "displayOrder": 0,
            "createdAt": "2026-11-21T09:12:03.120Z",
            "updatedAt": "2026-11-21T09:12:03.120Z",
            "children": [{
                "id": uuid(9),
                "name": "Garage",
                "parentId": "00000000-0000-0000-0000-000000000001",
                "displayOrder": 0,
                "createdAt": "2026-11-21T09:12:03.120Z",
                "updatedAt": "2026-11-21T09:12:03.120Z",
                "children": []
            }]
        }])
    );
}

#[test]
fn an_equipment_with_details_serialises_as_sowels_equipment_with_details() {
    let mut rig = garage();
    rig.now = T0 + 1000;
    rig.measure(&support::temperature(0x78, 12.5));
    rig.now = T0 + 2000;
    rig.measure(&support::state(0x7A, true));
    rig.now = T0 + 3000;
    let sensor = rig.get(&format!("/api/v1/equipments/{}", uuid(10))).1;
    let binding = |id: u8,
                   data: u8,
                   alias: &str,
                   kind: &str,
                   category: &str,
                   value: serde_json::Value,
                   unit: &str,
                   at: serde_json::Value| {
        json!({
            "id": uuid(id),
            "equipmentId": uuid(10),
            "deviceDataId": uuid(data),
            "alias": alias,
            "deviceId": uuid(1),
            "deviceName": "TH01 5678",
            "key": alias,
            "type": kind,
            "category": category,
            "value": value,
            "unit": unit,
            "lastUpdated": at,
            "lastChanged": at,
            "stale": false
        })
    };
    let at = json!("2026-11-21T09:12:04.120Z");
    let mut humidity = binding(
        12,
        3,
        "humidity",
        "number",
        "humidity",
        json!(null),
        "%",
        json!(null),
    );
    humidity["lastChanged"] = json!(null);
    let expected = json!({
        "id": uuid(10),
        "name": "Climate",
        "zoneId": uuid(9),
        "type": "sensor",
        "enabled": true,
        "createdAt": "2026-11-21T09:12:03.120Z",
        "updatedAt": "2026-11-21T09:12:03.120Z",
        "dataBindings": [
            binding(11, 2, "temperature", "number", "temperature", json!(12.5), "°C", at),
            humidity,
            binding(13, 4, "battery", "number", "battery", json!(null), "%", json!(null)),
            binding(14, 5, "voltage", "number", "voltage", json!(null), "mV", json!(null))
        ],
        "orderBindings": [],
        "status": "online"
    });
    assert_eq!(sensor, expected);

    let light = rig.get(&format!("/api/v1/equipments/{}", uuid(15))).1;
    assert_eq!(
        light,
        json!({
            "id": uuid(15),
            "name": "Light",
            "zoneId": uuid(9),
            "type": "light_onoff",
            "enabled": true,
            "createdAt": "2026-11-21T09:12:03.120Z",
            "updatedAt": "2026-11-21T09:12:03.120Z",
            "dataBindings": [{
                "id": uuid(16),
                "equipmentId": uuid(15),
                "deviceDataId": uuid(7),
                "alias": "state",
                "deviceId": uuid(6),
                "deviceName": "ZBMINIR2 567a",
                "key": "state",
                "type": "boolean",
                "category": "light_state",
                "value": true,
                "lastUpdated": "2026-11-21T09:12:05.120Z",
                "lastChanged": "2026-11-21T09:12:05.120Z",
                "stale": false
            }],
            "orderBindings": [{
                "id": uuid(17),
                "equipmentId": uuid(15),
                "deviceOrderId": uuid(8),
                "alias": "state",
                "deviceId": uuid(6),
                "deviceName": "ZBMINIR2 567a",
                "key": "state",
                "type": "boolean",
                "category": "light_toggle"
            }],
            "status": "online"
        })
    );
}

#[test]
fn an_equipment_that_is_not_online_has_its_status_reason_as_sowels() {
    let mut rig = garage();
    // The relay is mains powered: silent for 11 minutes, it is offline.
    rig.now = T0 + 11 * 60_000;
    let light = rig.get(&format!("/api/v1/equipments/{}", uuid(15))).1;
    assert_eq!(light["status"], "offline");
    assert_eq!(
        light["statusReason"],
        json!({
            "offlineDevices": ["ZBMINIR2 567a"],
            "staleBindings": [],
            "offlineSince": "2026-11-21T09:12:03.120Z"
        })
    );
}

#[test]
fn the_bindings_serialise_as_sowels_data_binding_and_order_binding() {
    let mut rig = garage();
    let body = rig
        .post(
            &format!("/api/v1/equipments/{}/data-bindings", uuid(10)),
            json!({"deviceDataId": uuid(7), "alias": "relay_state"}),
        )
        .1;
    // Ids: the next random bytes are 18.
    assert_eq!(
        body,
        json!({
            "id": uuid(18),
            "equipmentId": uuid(10),
            "deviceDataId": uuid(7),
            "alias": "relay_state"
        })
    );
    let body = rig
        .post(
            &format!("/api/v1/equipments/{}/order-bindings", uuid(10)),
            json!({"deviceOrderId": uuid(8), "alias": "toggle"}),
        )
        .1;
    assert_eq!(
        body,
        json!({
            "id": uuid(19),
            "equipmentId": uuid(10),
            "deviceOrderId": uuid(8),
            "alias": "toggle"
        })
    );
    // PUT answers Sowel's Equipment: no bindings, no status.
    let put = rig
        .put(
            &format!("/api/v1/equipments/{}", uuid(15)),
            json!({"name": "Light"}),
        )
        .1;
    assert_eq!(
        put,
        json!({
            "id": uuid(15),
            "name": "Light",
            "zoneId": uuid(9),
            "type": "light_onoff",
            "enabled": true,
            "createdAt": "2026-11-21T09:12:03.120Z",
            "updatedAt": "2026-11-21T09:12:03.120Z"
        })
    );
}

#[test]
fn the_aggregation_serialises_as_sowels_zone_aggregated_data() {
    let mut rig = garage();
    rig.now = T0 + 1000;
    rig.measure(&support::temperature(0x78, 12.5));
    rig.measure(&support::humidity(0x78, 60.0));
    rig.measure(&support::state(0x7A, true));
    let all = rig.get("/api/v1/zones/aggregation").1;
    let zone = json!({
        "temperature": 12.5, "humidity": 60, "luminosity": null,
        "motion": false, "motionSensors": 0, "motionSince": null,
        "openDoors": 0, "openWindows": 0, "waterLeak": false, "smoke": false,
        "lightsOn": 1, "lightsTotal": 1,
        "shuttersOpen": 0, "shuttersTotal": 0, "averageShutterPosition": null,
        "doorsOpen": 0, "doorsTotal": 0, "waterValvesOpen": 0, "waterValvesTotal": 0,
        "waterFlowTotal": null, "powerTotal": null,
        "sunrise": null, "sunset": null, "isDaylight": null,
        "displaysOnline": 0, "displaysTotal": 0,
        "unavailableEquipmentsByCategory": {},
        "unavailableEquipmentNamesByCategory": {}
    });
    assert_eq!(
        all,
        json!({
            "00000000-0000-0000-0000-000000000001": zone,
            uuid(9): zone
        })
    );
}

#[test]
fn an_unavailable_equipment_serialises_the_two_maps_of_sowels_type() {
    let mut rig = garage();
    rig.now = T0 + 11 * 60_000;
    let all = rig.get("/api/v1/zones/aggregation").1;
    let zone = &all[uuid(9)];
    assert_eq!(
        zone["unavailableEquipmentsByCategory"],
        json!({"light_state": 1})
    );
    assert_eq!(
        zone["unavailableEquipmentNamesByCategory"],
        json!({"light_state": ["Light"]})
    );
}

// ---- Spec 026: the settings' events, the root zone's sun data ----

#[test]
fn the_settings_and_sunlight_events_serialise_as_sowels_engine_event() {
    use solite_core::live::EngineEvent;
    // `{ type: "settings.changed"; keys: string[] }` and `{ type: "sunlight.changed" }`.
    let changed = EngineEvent::SettingsChanged {
        keys: vec!["home.latitude".into(), "home.longitude".into()],
    };
    assert_eq!(
        serde_json::to_string(&changed).unwrap(),
        r#"{"type":"settings.changed","keys":["home.latitude","home.longitude"]}"#
    );
    assert_eq!(
        serde_json::to_string(&EngineEvent::SunlightChanged).unwrap(),
        r#"{"type":"sunlight.changed"}"#
    );
}

#[test]
fn the_root_zone_carries_the_sun_data_of_sowels_zone_aggregated_data() {
    let mut rig = Rig::new();
    rig.now = 1_782_043_200_000; // 2026-06-21T12:00:00Z
    let (status, _) = rig.put(
        "/api/v1/settings",
        json!({
            "home.latitude": "48.8566",
            "home.longitude": "2.3522",
            "home.timezone": "Europe/Paris",
        }),
    );
    assert_eq!(status, 200);
    let text = rig
        .raw("GET", "/api/v1/zones/aggregation", "")
        .body
        .unwrap();
    assert!(
        text.contains(r#""sunrise":"05:46","sunset":"21:57","isDaylight":true"#),
        "{text}"
    );
}
