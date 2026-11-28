//! The checks beside the engine's worst case (`memory_worst_case.rs`), in their own process so
//! that their threads never run while the engine's peak is read: every `pub const MAX_*` of the
//! core and the recipe SDK is named in that file's header, and the script recipes pushed to their
//! memory limit (QuickJS's own count, per runtime).

mod worst_case;

use std::path::{Path, PathBuf};

use serde_json::json;
use solite_core::api::handle;
use solite_core::devices::{DeviceWithDetails, Devices, Id};
use solite_core::home::Home;
use solite_core::recipes::MAX_INSTANCES_PER_SCRIPT;
use worst_case::{T0, created_id, device, request};

/// Every `.rs` file under `dir`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_memory_bound_is_listed_in_the_header() {
    let header: String = include_str!("memory_worst_case.rs")
        .lines()
        .take_while(|l| l.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    // The core's own sources, named from `crates/core/src`; the recipe SDK and the recipes, named
    // from `crates/` (`recipe-sdk/src/bounds.rs`).
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let roots = [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        crates.join("recipe-sdk/src"),
    ];
    let mut missing = Vec::new();
    let mut found = 0;
    for root in roots {
        let base = if root.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
            root.clone()
        } else {
            crates.clone()
        };
        let mut files = Vec::new();
        sources(&root, &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let relative = file
                .strip_prefix(&base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            for line in text.lines() {
                let Some(rest) = line.trim_start().strip_prefix("pub const MAX_") else {
                    continue;
                };
                found += 1;
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                    .collect();
                let entry = format!("`{relative}::MAX_{name}`");
                if !header.contains(&entry) {
                    missing.push(entry);
                }
            }
        }
    }
    assert!(found > 20, "only {found} bounds found: the scan is broken");
    assert!(
        missing.is_empty(),
        "a `pub const MAX_*` is missing from the header of memory_worst_case.rs: {missing:?}"
    );
}

/// `MAX_SCRIPT_RECIPES` script recipes pushed to their memory limit, `MAX_INSTANCES_PER_SCRIPT`
/// instances in each (spec 041, R6.5): their memory, QuickJS's own, on its own line.
#[cfg(feature = "script-recipes")]
#[test]
fn scripts_pushed_to_their_limit_with_four_instances_each() {
    use solite_core::recipes::native::{NativeHost, RecipeName};
    use solite_core::recipes::script::{
        SYSTEM_MEMORY_LIMIT, ScriptRecipe, ScriptRuntimes, SystemRuntimes,
    };
    use solite_core::recipes::{MAX_SCRIPT_RECIPES, ScriptClock};
    use solite_recipe_sdk::{Clock, Recipe};
    use std::sync::Arc;

    let mut calls = 0u32;
    let mut random = move |buffer: &mut [u8]| -> Result<(), String> {
        calls += 1;
        buffer.fill(0x5A);
        buffer[..4].copy_from_slice(&calls.to_be_bytes());
        Ok(())
    };
    // A zone with 16 lights, for the instances.
    let mut counter = 0u32;
    let list: Vec<DeviceWithDetails> = (0..16).map(|i| device(i, &mut counter)).collect();
    let devices = Devices::from_list(list);
    let device_ids: Vec<String> = devices
        .list()
        .iter()
        .map(|d| d.device.id.to_string())
        .collect();
    let mut home = Home::from_devices(devices, T0);
    let created = handle(
        &mut home,
        &request("POST", "/api/v1/zones", br#"{"name":"Garage"}"#),
        T0,
        &mut random,
    );
    assert_eq!(created.status, 201, "{created:?}");
    let zone = created_id(&created);
    let lights: Vec<String> = device_ids
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let body = json!({
                "name": format!("Light {i}"), "type": "light_onoff",
                "zoneId": zone, "deviceIds": [device],
            })
            .to_string();
            let created = handle(
                &mut home,
                &request("POST", "/api/v1/equipments", body.as_bytes()),
                T0,
                &mut random,
            );
            assert_eq!(created.status, 201, "{created:?}");
            created_id(&created)
        })
        .collect();

    let runtimes: Arc<dyn ScriptRuntimes> = Arc::new(SystemRuntimes);
    let clock: ScriptClock = Arc::new(|| 0);
    let params = json!({"zone": zone, "lights": lights, "timeout": "20s", "maxOnDuration": "5m"});
    let script = include_str!("../../../recipes/scripts/motion-light.js");
    // What a script of this size holds with its instances, with no ballast.
    let per_recipe = MAX_INSTANCES_PER_SCRIPT;
    let names = |k: usize| RecipeName {
        id: format!("motion-light-{k}").into(),
        name: "Motion Light".into(),
    };
    let source = |k: usize, ballast: usize| {
        let text = script.replace("id: \"motion-light\"", &format!("id: \"motion-light-{k}\""));
        format!("{text}\nvar ballast = \"x\".repeat({ballast});\n")
    };
    let started = |k: usize, ballast: usize| -> ScriptRecipe {
        let (mut recipe, _) = ScriptRecipe::load(&source(k, ballast), &runtimes, &clock).unwrap();
        let name = names(k);
        let host = NativeHost::new(&home, Id::ROOT_ZONE, &name, T0, T0);
        for handle in 0..per_recipe {
            let clock = Clock { mono: T0, wall: T0 };
            recipe
                .start(&host, u32::try_from(handle).unwrap(), &params, clock)
                .unwrap();
        }
        recipe
    };
    let bare = started(0, 0).held_bytes();
    // The ballast takes each runtime to about 85 % of its limit (a runtime that still holds 90 %
    // after a collection has failed).
    let target = SYSTEM_MEMORY_LIMIT / 100 * 85;
    let ballast = target - bare;
    let recipes: Vec<ScriptRecipe> = (0..MAX_SCRIPT_RECIPES)
        .map(|k| started(k, ballast))
        .collect();
    let instances: usize = recipes.iter().map(ScriptRecipe::instance_count).sum();
    assert_eq!(instances, MAX_SCRIPT_RECIPES * MAX_INSTANCES_PER_SCRIPT);
    let held: Vec<usize> = recipes.iter().map(ScriptRecipe::held_bytes).collect();
    let total: usize = held.iter().sum();
    println!(
        "script memory: {MAX_SCRIPT_RECIPES} scripts at their limit, {instances} instances: {held:?}, {total} B in all (the Mac's limit is {SYSTEM_MEMORY_LIMIT} B each, the box's limit 224 KB each)"
    );
    for bytes in &held {
        assert!(*bytes <= SYSTEM_MEMORY_LIMIT / 100 * 90, "{held:?}");
        assert!(*bytes >= SYSTEM_MEMORY_LIMIT / 100 * 80, "{held:?}");
    }
    assert!(total < MAX_SCRIPT_RECIPES * SYSTEM_MEMORY_LIMIT, "{total}");

    // A script of about 32 KB of real code (spec 053), loaded with no ballast and 4 instances: the
    // source, its bytecode and the instances stay under one runtime's limit.
    let big = include_str!("fixtures/scripts/big-recipe.js");
    assert!(
        big.len() > 32_000 && big.len() <= 32 * 1024,
        "{}",
        big.len()
    );
    let (mut recipe, _) = ScriptRecipe::load(big, &runtimes, &clock).unwrap();
    let loaded = recipe.held_bytes();
    let name = RecipeName {
        id: "big-recipe".into(),
        name: "Big Recipe".into(),
    };
    let host = NativeHost::new(&home, Id::ROOT_ZONE, &name, T0, T0);
    for handle in 0..per_recipe {
        let clock = Clock { mono: T0, wall: T0 };
        recipe
            .start(&host, u32::try_from(handle).unwrap(), &params, clock)
            .unwrap();
    }
    assert_eq!(recipe.instance_count(), MAX_INSTANCES_PER_SCRIPT);
    let held = recipe.held_bytes();
    println!(
        "big script memory: {} B of source, loaded {loaded} B, {MAX_INSTANCES_PER_SCRIPT} instances {held} B (the limit is {SYSTEM_MEMORY_LIMIT} B)",
        big.len()
    );
    assert!(held < SYSTEM_MEMORY_LIMIT / 100 * 90, "{held}");
}
