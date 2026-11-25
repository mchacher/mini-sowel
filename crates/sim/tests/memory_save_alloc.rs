//! The memory file is written in a stream, not built whole in a growing text. One test file with
//! a counting allocator, so it sees nothing else: a text over 32 KB made `to_string_pretty` grow
//! its buffer to 64 KB and hold 32 + 64 KB while copying, which pushed the engine's peak in the
//! firmware's simulated run over D27's ceiling (spec 031 added the colour fields).

use std::fs;

use peak_alloc::PeakAlloc;
use solite_sim::memory;

mod support;
use support::scratch;

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

fn device(i: usize) -> String {
    format!(
        r#"{{"eui64Wire":"78563412004B{i:04X}","nwk":{n},"joined":true,"bound":true,
        "configured":[6,8,768],"configuredAttributes":[[6,0],[8,0],[768,3]],"reportsSent":true,
        "relayOn":false,"cieWritten":false,"enrolled":false,"cie":"","level":0,
        "colorTemp":370,"colorX":24939,"colorY":24701,"hue":0,"saturation":0,
        "silent":false,"lookupUnknown":false}}"#,
        n = 1000 + i
    )
}

#[test]
fn saving_the_memory_file_streams_it_and_writes_the_same_text() {
    let dir = scratch();
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.json");
    let devices: Vec<String> = (0..100).map(device).collect();
    fs::write(
        &source,
        format!(r#"{{"network":null,"devices":[{}]}}"#, devices.join(",")),
    )
    .unwrap();
    let loaded = memory::load(&source).unwrap().unwrap();
    let expected = serde_json::to_string_pretty(&loaded).unwrap();
    assert!(expected.len() > 40 * 1024, "the text must pass 32 KB");

    let target = dir.join("sim-save.json");
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();
    memory::save(&target, &loaded).unwrap();
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);

    assert_eq!(fs::read_to_string(&target).unwrap(), expected);
    // A small buffer and a path or two: far under the text itself.
    assert!(
        peak < 16 * 1024,
        "save peaked at {peak} bytes for a text of {} bytes",
        expected.len()
    );
}
