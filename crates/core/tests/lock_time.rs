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

// ---- Script recipes (spec 041, R6.1) ----

/// The realistic home with `count` instances of `recipe` (the three of `motion-light` the builder
/// made are deleted first), each on the first zone and its 16 lights, with the safety auto-off
/// (the realistic home's button is a sensor, which the script's slot refuses: no button).
#[cfg(feature = "script-recipes")]
fn with_instances(recipe: &str, count: usize) -> realistic::Realistic {
    use realistic::request;
    use std::sync::Arc;

    // The system's allocator with a roomy limit: sixteen instances of the script hold more than
    // the Mac's own limit (`SystemRuntimes`).
    struct Roomy;
    impl solite_core::recipes::script::ScriptRuntimes for Roomy {
        fn new_runtime(&self) -> Result<solite_core::recipes::script::NewRuntime, String> {
            solite_core::recipes::script::SystemRuntimes.new_runtime()
        }
        fn memory_limit(&self) -> usize {
            1024 * 1024
        }
    }

    let mut random = realistic::random();
    let mut built = realistic::build(&mut random);
    for id in built.instance_ids.drain(..) {
        let path = format!("/api/v1/recipe-instances/{id}");
        let gone = solite_core::api::handle(
            &mut built.home,
            &request("DELETE", &path, b""),
            T0,
            &mut random,
        );
        assert_eq!(gone.status, 204);
    }
    let start = Instant::now();
    let clock: solite_core::recipes::ScriptClock =
        Arc::new(move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
    built
        .home
        .load_script_recipe(
            include_str!("../../../recipes/scripts/motion-light-script.js"),
            Arc::new(Roomy),
            clock,
        )
        .unwrap();
    for _ in 0..count {
        let body = serde_json::json!({
            "recipeId": recipe,
            "params": {"zone": built.zone_ids[0], "lights": built.lights,
                       "timeout": "20s", "maxOnDuration": "5m"}
        })
        .to_string();
        let created = solite_core::api::handle(
            &mut built.home,
            &request("POST", "/api/v1/recipe-instances", body.as_bytes()),
            T0,
            &mut random,
        );
        assert_eq!(created.status, 201, "{created:?}");
    }
    built
}

#[cfg(feature = "script-recipes")]
fn motion_batches(home: &mut Home) -> (f64, f64) {
    let hub = home.live();
    let client = hub.connect().expect("a free slot");
    assert!(hub.subscribe(
        client,
        r#"{"type":"subscribe","topics":["devices","equipments","zones","recipes"]}"#
    ));
    let mut now = T0;
    time_turns(
        home,
        client,
        |turn| {
            let sensor = [10u8, 20, 30][turn % 3];
            report(sensor, "motion", MeasureValue::Bool(turn % 2 == 0), "")
        },
        &mut now,
    )
}

#[cfg(feature = "script-recipes")]
#[test]
#[ignore = "slow: a measurement, run in release with --nocapture; figures in docs/memory-budget.md"]
fn a_motion_event_through_the_script_against_the_built_in_recipe() {
    for (recipe, label) in [
        ("motion-light", "built in"),
        ("motion-light-script", "script"),
    ] {
        let mut built = with_instances(recipe, 3);
        let (mean, longest) = motion_batches(&mut built.home);
        println!(
            "lock per motion batch, 3 instances of {recipe} ({label}): mean {mean:.1} us, longest {longest:.1} us"
        );
    }
}

#[cfg(feature = "script-recipes")]
#[test]
#[ignore = "slow: a measurement, run in release with --nocapture; figures in docs/memory-budget.md"]
fn one_motion_event_to_the_most_instances() {
    // The built-in recipe up to the engine's 16, a script recipe up to its 8.
    for (recipe, label, count) in [
        ("motion-light", "built in", 16),
        (
            "motion-light-script",
            "script",
            solite_core::recipes::MAX_INSTANCES_PER_SCRIPT,
        ),
    ] {
        let mut built = with_instances(recipe, count);
        let (mean, longest) = motion_batches(&mut built.home);
        println!(
            "lock per motion batch, {count} instances of {recipe} ({label}): mean {mean:.1} us, longest {longest:.1} us"
        );
    }
}
