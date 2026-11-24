//! How long the engine holds the home lock for one batch of measures (spec 027, R5): the realistic
//! home of decision D20, one WebSocket client listening to everything, and one turn of the
//! engine's loop under the lock (`measures`, `recompute`, `tick`, `run_recipes`, as
//! `engine::run` does) for a temperature report and for a motion report. A measurement, not a
//! gate: run it in release on the Mac and copy the figures into `docs/memory-budget.md`.
//!
//! ```text
//! cargo test --release -p solite-core --test lock_time -- --ignored --nocapture
//! ```

use std::time::Instant;

use solite_core::home::Home;
use solite_core::radio::measures::{Measure, Value as MeasureValue};
use solite_core::radio::{MeasureEvent, Sender};

mod realistic;

use realistic::T0;

const TURNS: usize = 500;

fn report(device: u8, kind: &'static str, value: MeasureValue, unit: &'static str) -> MeasureEvent {
    MeasureEvent {
        sender: Sender::Eui64([0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, device]),
        endpoint: 1,
        measures: vec![Measure {
            kind,
            value,
            unit,
            device_value: None,
        }],
    }
}

/// The time of one turn under the lock, in microseconds: the mean and the longest of `TURNS`.
fn time_turns(
    home: &mut Home,
    client: solite_core::live::ClientId,
    make: impl Fn(usize) -> MeasureEvent,
    now: &mut u64,
) -> (f64, f64) {
    let mut random = realistic::random();
    let hub = home.live();
    let mut total = 0f64;
    let mut longest = 0f64;
    for turn in 0..TURNS {
        *now += 1000;
        let event = make(turn);
        let start = Instant::now();
        home.measures(&event, *now, &mut random).unwrap();
        home.recompute(*now);
        home.tick(*now);
        home.run_recipes(*now, *now);
        let spent = start.elapsed().as_secs_f64() * 1e6;
        total += spent;
        longest = longest.max(spent);
        // The client reads its queue (outside the lock in the engine), so it never overflows.
        while hub.pop(client).is_some() {}
    }
    (total / TURNS as f64, longest)
}

#[test]
#[ignore = "slow: a measurement, run in release with --nocapture; figures in docs/memory-budget.md"]
fn one_batch_of_measures_holds_the_home_lock() {
    let mut random = realistic::random();
    let mut built = realistic::build(&mut random);
    let hub = built.home.live();
    let client = hub.connect().expect("a free slot");
    assert!(hub.subscribe(
        client,
        r#"{"type":"subscribe","topics":["devices","equipments","zones","recipes"]}"#
    ));
    let mut now = T0;
    let (mean, longest) = time_turns(
        &mut built.home,
        client,
        |turn| {
            report(
                (turn % 50) as u8,
                "temperature",
                MeasureValue::Number(19.0 + (turn % 10) as f64 / 10.0),
                "°C",
            )
        },
        &mut now,
    );
    println!("lock per temperature batch: mean {mean:.1} us, longest {longest:.1} us");
    let (mean, longest) = time_turns(
        &mut built.home,
        client,
        |turn| {
            let sensor = [10u8, 20, 30][turn % 3];
            report(sensor, "motion", MeasureValue::Bool(turn % 2 == 0), "")
        },
        &mut now,
    );
    println!("lock per motion batch: mean {mean:.1} us, longest {longest:.1} us");
}
