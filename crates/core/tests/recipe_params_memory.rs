//! Spec 012 R7.1, R8.5: an instance keeps its `params` as the JSON text received (at most 2 KB),
//! never as a parsed value: 2 KB of nested JSON such as `{"0":{"a":0},"1":{"a":0},...}` takes
//! about 112 KB once parsed, so 16 instances would hold about 1.8 MB. One test only in this file,
//! so the counting allocator sees nothing else.

use peak_alloc::PeakAlloc;
use solite_core::api::{Reach, Reply, Request, handle, handle_stream};
use solite_core::devices::Devices;
use solite_core::home::Home;

#[global_allocator]
static ALLOCATOR: PeakAlloc = PeakAlloc;

const T0: u64 = 1_795_252_323_120;
/// 16 instances x about 2.5 KB of params, plus their start-up log line and runner: well under
/// 16 x 4 KB. A parsed value would hold about 1.8 MB.
const KEPT_BOUND: usize = 16 * 4 * 1024;
const PEAK_BOUND: usize = 300 * 1024;

fn request<'a>(method: &'a str, path: &'a str, body: &'a [u8]) -> Request<'a> {
    Request {
        method,
        path,
        host: Some("127.0.0.1:8080"),
        port: 8080,
        reach: Reach::Loopback,
        content_type: Some("application/json"),
        authorization: None,
        content_length: None,
        body,
    }
}

#[test]
fn sixteen_instances_with_2_kb_of_nested_params_keep_only_their_text() {
    let mut calls = 0u32;
    let mut random = move |buffer: &mut [u8]| -> Result<(), String> {
        calls += 1;
        buffer.fill(0x5A);
        buffer[..4].copy_from_slice(&calls.to_be_bytes());
        Ok(())
    };
    let mut devices = Devices::new();
    let eui = [0x00, 0x12, 0x4B, 0x00, 0x12, 0x34, 0x56, 1];
    devices
        .identified(eui, Some("Manufacturer"), Some("Model"), T0, &mut random)
        .unwrap();
    devices
        .reporting_configured(eui, &[0x0006], T0, &mut random)
        .unwrap();
    let device = devices.list()[0].device.id.to_string();
    let mut home = Home::from_devices(devices, T0);
    let zone = handle(
        &mut home,
        &request("POST", "/api/v1/zones", br#"{"name":"Garage"}"#),
        T0,
        &mut random,
    );
    let zone: serde_json::Value = serde_json::from_str(&zone.body.unwrap()).unwrap();
    let zone = zone["id"].as_str().unwrap().to_string();
    let body = format!(
        r#"{{"name":"Light","type":"light_onoff","zoneId":"{zone}","deviceIds":["{device}"]}}"#
    );
    let light = handle(
        &mut home,
        &request("POST", "/api/v1/equipments", body.as_bytes()),
        T0,
        &mut random,
    );
    assert_eq!(light.status, 201, "{light:?}");
    let light: serde_json::Value = serde_json::from_str(&light.body.unwrap()).unwrap();
    let light = light["id"].as_str().unwrap().to_string();

    // A valid `zone` and `lights`, then nested objects up to the 2 KB limit.
    let mut params = format!(r#"{{"zone":"{zone}","lights":["{light}"]"#);
    let mut i = 0;
    while params.len() + 20 < 2048 {
        params.push_str(&format!(r#","{i}":{{"a":0}}"#));
        i += 1;
    }
    params.push('}');
    assert!(
        params.len() <= 2048 && params.len() > 2000,
        "{}",
        params.len()
    );
    // A script recipe holds 4 instances (D85): the script and three copies under other ids.
    let script = include_str!("../../../recipes/scripts/motion-light.js");
    let recipes = [
        "motion-light",
        "motion-light-b",
        "motion-light-c",
        "motion-light-d",
    ];
    for id in recipes {
        let clock: solite_core::recipes::ScriptClock = std::sync::Arc::new(|| 0);
        home.load_script_recipe(
            &script.replace("id: \"motion-light\"", &format!("id: \"{id}\"")),
            std::sync::Arc::new(solite_core::recipes::script::SystemRuntimes),
            clock,
        )
        .unwrap();
    }

    let base = ALLOCATOR.current_usage();
    ALLOCATOR.reset_peak_usage();
    for n in 0..16 {
        let body = format!(r#"{{"recipeId":"{}","params":{params}}}"#, recipes[n / 4]);
        let created = handle(
            &mut home,
            &request("POST", "/api/v1/recipe-instances", body.as_bytes()),
            T0,
            &mut random,
        );
        assert_eq!(created.status, 201, "{created:?}");
    }
    let kept = ALLOCATOR.current_usage().saturating_sub(base);
    // The list is streamed and the log read, as the server does.
    let reply = handle_stream(
        &mut home,
        &request("GET", "/api/v1/recipe-instances", b""),
        T0,
        &mut random,
    );
    let Reply::List(mut cursor) = reply else {
        panic!("recipe instances are streamed");
    };
    while cursor.next_chunk(&home).unwrap().is_some() {}
    let peak = ALLOCATOR.peak_usage().saturating_sub(base);

    let text_bytes: usize = home.recipes().instances().map(|i| i.params.len()).sum();
    assert_eq!(home.recipes().len(), 16);
    assert!(text_bytes <= 16 * 2048, "{text_bytes}");
    println!(
        "recipe params: 16 instances, params text {text_bytes} bytes, kept {kept} bytes ({} KB), peak {peak} bytes ({} KB), kept bound {KEPT_BOUND}, peak bound {PEAK_BOUND}",
        kept / 1024,
        peak / 1024
    );
    assert!(kept < KEPT_BOUND, "kept {kept} bytes");
    assert!(peak < PEAK_BOUND, "peak {peak} bytes");
}
