//! Memory ceiling of the devices (spec 008, R5). One test only in this file, so the counting
//! allocator sees nothing else: a home of 50 devices with 6 data and 1 order each, the list
//! serialised as the API serves it, then as the save path writes it. The Mac's 64-bit figure is an upper bound of the ESP32's.

use mini_sowel_core::api::{Request, handle};
use mini_sowel_core::devices::Devices;
use peak_alloc::PeakAlloc;

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

const CEILING_BYTES: usize = 200 * 1024;
const T0: u64 = 1_795_252_323_120;

#[test]
fn fifty_devices_with_six_data_and_one_order_serialise_under_200_kb() {
    let mut calls = 0u8;
    let mut random = move |buffer: &mut [u8]| -> Result<(), String> {
        calls = calls.wrapping_add(1);
        buffer.fill(calls);
        buffer[0] = calls.wrapping_mul(7);
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
    let response = handle(
        &mut devices,
        &Request {
            method: "GET",
            path: "/api/v1/devices",
            host: Some("127.0.0.1:8080"),
            port: 8080,
            body: b"",
        },
        T0,
    );
    let body = response.body.expect("a body");
    assert_eq!(response.status, 200);
    let body_len = body.len();
    // The API answer is sent and gone before the next save: not two buffers at once.
    drop(body);
    // The save path: the same list, serialised under the lock into the bytes that are written.
    let saved = devices.to_json().expect("the devices serialise");
    assert_eq!(saved.len(), body_len);
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);
    println!(
        "memory ceiling: peak {peak} bytes ({} KB), body {body_len} bytes, ceiling {CEILING_BYTES}",
        peak / 1024
    );
    assert!(peak < CEILING_BYTES, "peak {peak} bytes");
}
