//! Memory ceiling of the home (spec 008 R5, spec 010 R7.15). One test only in this file, so the
//! counting allocator sees nothing else: a home of 50 devices with 6 data and 1 order each, 10
//! zones and 50 equipments of 6 bindings, then the largest single response being built
//! (`GET /equipments`, one answer at a time, never all at once), then the save path. The
//! Mac's 64-bit figure is an upper bound of the ESP32's.

use peak_alloc::PeakAlloc;
use solite_core::api::{Reply, handle, handle_stream};
use solite_core::home::{Home, ListCursor};
use solite_core::radio::measures::{Measure, Value as MeasureValue};
use solite_core::radio::{MeasureEvent, Sender};

mod realistic;

use realistic::{Realistic, T0, request};

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

/// The engine's budget for a realistic home, decision D20.
const CEILING_BYTES: usize = 300 * 1024;

#[test]
fn a_home_of_fifty_devices_ten_zones_and_fifty_equipments_answers_under_300_kb() {
    let mut random = realistic::random();
    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();

    let Realistic {
        mut home,
        lights: _,
        instance_ids,
        ..
    } = realistic::build(&mut random);
    // The lights keep their bindings: since spec 024 a light that loses its toggle binding
    // disables the instance (R4), so the long error lines of a missing binding no longer come;
    // the logs fill with the switching lines, and the worst case (every entry at 200 bytes) is
    // added to the peak below.
    // The button is pressed (a voltage report) until every log is full.
    let press = MeasureEvent {
        sender: Sender::Eui64([0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 0]),
        endpoint: 1,
        measures: vec![Measure {
            kind: "voltage",
            value: MeasureValue::Number(3000.0),
            unit: "mV",
            device_value: None,
        }],
    };
    let mut now = T0;
    for _ in 0..60 {
        now += 1000;
        home.measures(&press, now, &mut random).unwrap();
        home.recompute(now);
        home.run_recipes(now, now);
    }
    // The queue of events full: more reports than it holds, not handed over.
    for _ in 0..130 {
        home.measures(&press, now, &mut random).unwrap();
    }
    assert_eq!(home.events().len(), 128);
    // Spec 013: one client of the WebSocket, listening to everything, with its queue full (at
    // the bound, a report from a new device away from overflowing) and a batch being written:
    // an event taken from the queue and held, while the queue fills up again. The client stays
    // through the answers and the save path below, the heaviest moments.
    let hub = home.live();
    let client = hub.connect().expect("a free slot");
    assert!(hub.subscribe(
        client,
        r#"{"type":"subscribe","topics":["devices","equipments","zones","recipes"]}"#
    ));
    let fill_queue = |home: &mut Home, now: u64, from: u8| {
        let mut device = from;
        while hub.queued(client).1 + 750 <= solite_core::live::MAX_QUEUED_BYTES {
            let report = MeasureEvent {
                sender: Sender::Eui64([0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, device]),
                endpoint: 1,
                measures: vec![Measure {
                    kind: "voltage",
                    value: MeasureValue::Number(2900.0 + f64::from(device)),
                    unit: "mV",
                    device_value: None,
                }],
            };
            home.measures(&report, now, &mut |b: &mut [u8]| {
                b.fill(0x33);
                Ok(())
            })
            .unwrap();
            device += 1;
            assert!(device < 50, "the queue never filled");
        }
    };
    fill_queue(&mut home, now, 1);
    assert_eq!(hub.close_request(client), None, "not overflowed");
    let (queued_events, queued_bytes) = hub.queued(client);
    assert!(
        queued_bytes + 750 > solite_core::live::MAX_QUEUED_BYTES,
        "the queue is full: {queued_bytes} bytes"
    );
    assert_eq!(hub.begin_batch(client), queued_events);
    let in_flight = hub.pop(client).expect("an event to write");
    fill_queue(&mut home, now, 30);
    assert_eq!(hub.close_request(client), None, "not overflowed");
    let (queued_events, queued_bytes) = hub.queued(client);

    // The logs: 50 entries each, of switching lines. The worst case (every entry 200
    // bytes) is added to the peak to show that it fits too.
    let mut log_bytes = 0usize;
    for id in &instance_ids {
        let log = home.recipes().log(id, 50);
        assert_eq!(log.len(), 50);
        log_bytes += log.iter().map(|e| e.message.len() + 56).sum::<usize>();
    }
    let worst_log_bytes: usize = 3 * 50 * (200 + 56);

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
    // The recipes' list and file, one chunk at a time like the others.
    let mut recipes_listed = 0;
    let mut cursor = ListCursor::recipe_instances(&home);
    while let Some(chunk) = cursor.next_chunk(&home).unwrap() {
        recipes_listed += chunk.len();
    }
    let mut cursor = home
        .take_recipes_cursor()
        .expect("the recipes are to be written");
    let mut recipes_saved = 0;
    while let Some(chunk) = cursor.next_chunk(&home).unwrap() {
        recipes_saved += chunk.len();
    }
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);
    let worst = peak + worst_log_bytes.saturating_sub(log_bytes);
    // The client's last batch is taken after the measure, with the queue still full.
    assert!(in_flight.len() > 50);
    assert_eq!(hub.close_request(client), None);
    println!(
        "memory ceiling: WebSocket client queue {queued_events} events, {queued_bytes} bytes, one event of {} bytes in flight; peak {peak} bytes ({} KB), with every log entry at 200 bytes {worst} bytes ({} KB), logs {log_bytes} bytes, GET /recipe-instances {recipes_listed} bytes, recipes.json {recipes_saved} bytes, GET /equipments {body_len} bytes (streamed), GET /devices {} bytes (streamed), devices.json {saved_devices} bytes, zones.json {saved_zones} bytes, equipments.json {saved_equipments} bytes, ceiling {CEILING_BYTES}",
        in_flight.len(),
        peak / 1024,
        worst / 1024,
        sizes[1],
    );
    assert!(peak < CEILING_BYTES, "peak {peak} bytes");
    assert!(worst < CEILING_BYTES, "worst case {worst} bytes");
}
