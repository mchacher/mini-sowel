//! Memory held by the devices read back from storage (spec 034). One test only in this file, so
//! the counting allocator sees nothing else: the realistic home's `devices.json`
//! (`firmware/fixtures/`, 50 devices of 6 data and 1 order) is read the way the engine reads it at
//! boot, then the bytes it holds and the peak of the read are printed and checked against a bound.
//! The Mac's 64-bit figure is an upper bound of the ESP32's.

use std::path::PathBuf;

use peak_alloc::PeakAlloc;
use solite_core::devices::{Devices, parse_list};

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

/// What the 50 devices may hold once read (spec 034 R2.4): the figure measured after the texts
/// are shared (67450 bytes), plus about 6 %: below the 73938 bytes held before, so undoing the
/// sharing fails this test.
const HELD_BOUND_BYTES: usize = 70 * 1024;

#[test]
fn the_realistic_devices_read_back_hold_under_their_bound() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../firmware/fixtures/devices.json");
    let text = std::fs::read_to_string(path).unwrap();
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();

    let devices = Devices::from_list(parse_list(&text).unwrap());

    let held = ALLOCATOR.current_usage().saturating_sub(base);
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);
    assert_eq!(devices.list().len(), 50);
    println!(
        "devices read back: {} devices, held {held} bytes ({} KB), peak of the read {peak} bytes ({} KB), devices.json {} bytes",
        devices.list().len(),
        held / 1024,
        peak / 1024,
        text.len(),
    );
    assert!(held < HELD_BOUND_BYTES, "held {held} bytes");
}
