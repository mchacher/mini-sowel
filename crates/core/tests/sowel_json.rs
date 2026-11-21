//! A device serialised equals an example written by hand from Sowel's `types.ts`
//! (`Device` T:185, `DeviceData` T:225, `DeviceOrder` T:236, `DeviceWithDetails` T:259): field
//! names, camelCase, enum values, `null` and omitted fields. The example is not produced by the
//! code under test.

use serde_json::json;

use mini_sowel_core::devices::Devices;

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
        "integrationId": "mini-sowel",
        "sourceDeviceId": "0x00124b0012345678",
        "name": "TH01 5678",
        "manufacturer": "eWeLink",
        "model": "TH01",
        "ieeeAddress": "0x00124b0012345678",
        "zoneId": null,
        "source": "mini-sowel",
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
