//! Memory ceiling of the home (spec 008 R5, spec 010 R7.15). One test only in this file, so the
//! counting allocator sees nothing else: a home of 50 devices with 6 data and 1 order each, 10
//! zones and 50 equipments of 6 bindings, then the largest single response being built
//! (`GET /equipments`, one answer at a time, never all at once), then the save path. The
//! Mac's 64-bit figure is an upper bound of the ESP32's.

use mini_sowel_core::api::{Reply, Request, handle, handle_stream};
use mini_sowel_core::devices::Devices;
use mini_sowel_core::home::{Home, ListCursor};
use peak_alloc::PeakAlloc;

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

const CEILING_BYTES: usize = 200 * 1024;
const T0: u64 = 1_795_252_323_120;

fn request<'a>(method: &'a str, path: &'a str, body: &'a [u8]) -> Request<'a> {
    Request {
        method,
        path,
        host: Some("127.0.0.1:8080"),
        port: 8080,
        content_type: Some("application/json"),
        body,
    }
}

#[test]
fn a_home_of_fifty_devices_ten_zones_and_fifty_equipments_answers_under_200_kb() {
    // Distinct ids: a counter in the first bytes, the rest filled.
    let mut calls = 0u32;
    let mut random = move |buffer: &mut [u8]| -> Result<(), String> {
        calls += 1;
        buffer.fill(0x5A);
        buffer[..4].copy_from_slice(&calls.to_be_bytes());
        Ok(())
    };
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();

    let mut devices = Devices::new();
    for i in 0..50u8 {
        let eui = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, i];
        devices
            .identified(eui, Some("Manufacturer"), Some("Model"), T0, &mut random)
            .unwrap();
        // Temperature, humidity, pressure, battery and voltage (5), and the on/off state (1)
        // with its order: 6 data and 1 order.
        devices
            .reporting_configured(
                eui,
                &[0x0402, 0x0405, 0x0403, 0x0001, 0x0006],
                T0,
                &mut random,
            )
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
            &mut random,
        );
        assert_eq!(created.status, 201);
        let zone: serde_json::Value = serde_json::from_str(&created.body.unwrap()).unwrap();
        zone_ids.push(zone["id"].as_str().unwrap().to_string());
    }
    for (i, device) in device_ids.iter().enumerate() {
        let body = format!(
            r#"{{"name":"Equipment number {i}","type":"sensor","zoneId":"{}","deviceIds":["{device}"]}}"#,
            zone_ids[i % 10]
        );
        let created = handle(
            &mut home,
            &request("POST", "/api/v1/equipments", body.as_bytes()),
            T0,
            &mut random,
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

    // The long lists are streamed, as the server does: one chunk alive at a time, taken and
    // dropped before the next. Only the length is kept.
    let mut sizes = Vec::new();
    for path in ["/api/v1/equipments", "/api/v1/devices"] {
        let reply = handle_stream(&mut home, &request("GET", path, b""), T0, &mut random);
        let Reply::List(mut cursor) = reply else {
            panic!("{path} is streamed");
        };
        let mut total = 0;
        while let Some(chunk) = cursor.next_chunk(&home).unwrap() {
            total += chunk.len();
        }
        sizes.push(total);
    }
    let body_len = sizes[0];
    // The other answers are whole, one at a time (each is sent and gone before the next).
    for path in ["/api/v1/zones", "/api/v1/zones/aggregation"] {
        let answer = handle(&mut home, &request("GET", path, b""), T0, &mut random);
        assert_eq!(answer.status, 200, "{path}");
    }
    // The save path: each file is written chunk by chunk from a cursor, as the host's saver
    // does; one chunk alive at a time.
    let mut sizes_saved = [0usize; 3];
    let mut cursors = [
        ListCursor::saved_devices(&home),
        home.take_zones_cursor()
            .expect("the zones are to be written"),
        home.take_equipments_cursor()
            .expect("the equipments are to be written"),
    ];
    for (size, cursor) in sizes_saved.iter_mut().zip(cursors.iter_mut()) {
        while let Some(chunk) = cursor.next_chunk(&home).unwrap() {
            *size += chunk.len();
        }
    }
    let (saved_devices, saved_zones, saved_equipments) =
        (sizes_saved[0], sizes_saved[1], sizes_saved[2]);
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);
    println!(
        "memory ceiling: peak {peak} bytes ({} KB), GET /equipments {body_len} bytes (streamed), GET /devices {} bytes (streamed), devices.json {saved_devices} bytes, zones.json {saved_zones} bytes, equipments.json {saved_equipments} bytes, ceiling {CEILING_BYTES}",
        peak / 1024,
        sizes[1],
    );
    assert!(peak < CEILING_BYTES, "peak {peak} bytes");
}
