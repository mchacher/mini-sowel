//! Sowel's device routes as a pure function.

use solite_core::api::{MAX_BODY_BYTES, Reach, Request, Response, handle};
use solite_core::devices::Devices;
use solite_core::home::Home;

const T0: u64 = 1_795_252_323_120;
const PORT: u16 = 8080;
const HOST: &str = "127.0.0.1:8080";

fn counter() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        buffer.fill(n);
        Ok(())
    }
}

/// Two devices, the second added a second later: a SNZB-02 and a relay.
fn two_devices() -> (Home, String, String) {
    let mut devices = Devices::new();
    let mut random = counter();
    let th = [0, 0x12, 0x4B, 0, 0x12, 0x34, 0x56, 0x78];
    let relay = [0, 0x12, 0x4B, 0, 0x12, 0x34, 0x56, 0x7A];
    devices
        .identified(th, Some("eWeLink"), Some("TH01"), T0, &mut random)
        .unwrap();
    devices
        .reporting_configured(th, &[0x0001, 0x0405, 0x0402], T0, &mut random)
        .unwrap();
    devices
        .identified(
            relay,
            Some("SONOFF"),
            Some("ZBMINIR2"),
            T0 + 1000,
            &mut random,
        )
        .unwrap();
    devices
        .reporting_configured(relay, &[0x0006], T0 + 1000, &mut random)
        .unwrap();
    let ids = devices
        .list()
        .iter()
        .map(|d| d.device.id.to_string())
        .collect::<Vec<_>>();
    (
        Home::from_devices(devices, T0),
        ids[0].clone(),
        ids[1].clone(),
    )
}

fn call(home: &mut Home, method: &str, path: &str, host: Option<&str>, body: &[u8]) -> Response {
    handle(
        home,
        &Request {
            method,
            path,
            host,
            port: PORT,
            reach: Reach::Loopback,
            content_type: Some("application/json"),
            authorization: None,
            body,
        },
        T0 + 2000,
        &mut counter(),
    )
}

fn local(devices: &mut Home, method: &str, path: &str, body: &str) -> Response {
    call(devices, method, path, Some(HOST), body.as_bytes())
}

fn json(response: &Response) -> serde_json::Value {
    serde_json::from_str(response.body.as_deref().expect("a body")).unwrap()
}

fn error(response: &Response) -> (u16, String) {
    (
        response.status,
        json(response)["error"].as_str().unwrap().to_string(),
    )
}

#[test]
fn get_list_is_sorted_by_created_at_with_the_data_in_table_order() {
    let (devices, th, relay) = two_devices();
    // Stored the other way round: the answer is sorted by createdAt anyway.
    let mut list = devices.devices.snapshot();
    list.reverse();
    let mut devices = Home::from_devices(Devices::from_list(list), T0);
    let response = local(&mut devices, "GET", "/api/v1/devices", "");
    assert_eq!(response.status, 200);
    let body = json(&response);
    let ids: Vec<_> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [th.as_str(), relay.as_str()]);
    let keys: Vec<_> = body[0]["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["temperature", "humidity", "battery", "voltage"]);
}

#[test]
fn get_by_id_answers_the_device_and_an_unknown_id_is_404() {
    let (mut devices, th, _) = two_devices();
    let found = local(&mut devices, "GET", &format!("/api/v1/devices/{th}"), "");
    assert_eq!(found.status, 200);
    assert_eq!(json(&found)["id"], th.as_str());
    assert_eq!(json(&found)["orders"], serde_json::json!([]));
    let missing = local(&mut devices, "GET", "/api/v1/devices/nope", "");
    assert_eq!(error(&missing), (404, "Device not found".to_string()));
    assert_eq!(
        missing.body.as_deref(),
        Some(r#"{"error":"Device not found"}"#)
    );
}

#[test]
fn put_checks_the_body_then_renames() {
    let (mut devices, th, _) = two_devices();
    let path = format!("/api/v1/devices/{th}");
    let put = |devices: &mut Home, body: &str| local(devices, "PUT", &path, body);
    let ok = put(&mut devices, r#"{"name":"Garage"}"#);
    assert_eq!(ok.status, 200);
    let body = json(&ok);
    assert_eq!(body["name"], "Garage");
    assert!(
        body.get("data").is_none(),
        "PUT answers a Device, not its details"
    );
    assert_eq!(body["updatedAt"], "2026-11-21T09:12:05.120Z");
    assert_eq!(
        devices.devices.get(&th).unwrap().device.name.as_ref(),
        "Garage"
    );

    let bad = |devices: &mut Home, body: &str| error(&put(devices, body));
    assert_eq!(bad(&mut devices, "{}"), (400, "Nothing to update".into()));
    assert_eq!(
        bad(&mut devices, r#"{"name":""}"#),
        (400, "Invalid name".into())
    );
    let long = format!(r#"{{"name":"{}"}}"#, "a".repeat(65));
    assert_eq!(bad(&mut devices, &long), (400, "Invalid name".into()));
    assert_eq!(
        bad(&mut devices, r#"{"x":1}"#),
        (400, "Unknown field: x".into())
    );
    assert_eq!(
        bad(&mut devices, r#"{"zoneId":"x"}"#),
        (404, "Zone not found".into())
    );
    assert_eq!(
        bad(&mut devices, r#"{"zoneId":5}"#),
        (400, "Invalid field: zoneId".into())
    );
    assert_eq!(bad(&mut devices, "not json"), (400, "Invalid JSON".into()));
    // Nothing changed by the refused ones.
    assert_eq!(
        devices.devices.get(&th).unwrap().device.name.as_ref(),
        "Garage"
    );

    let zone = put(&mut devices, r#"{"zoneId":null}"#);
    assert_eq!(zone.status, 200);
    assert!(json(&zone)["zoneId"].is_null());
    let unknown = local(
        &mut devices,
        "PUT",
        "/api/v1/devices/nope",
        r#"{"name":"A"}"#,
    );
    assert_eq!(error(&unknown), (404, "Device not found".into()));
}

#[test]
fn delete_answers_204_without_a_body_and_an_unknown_id_is_404() {
    let (mut devices, th, relay) = two_devices();
    let done = local(&mut devices, "DELETE", &format!("/api/v1/devices/{th}"), "");
    assert_eq!(
        done,
        Response {
            status: 204,
            body: None
        }
    );
    assert_eq!(devices.devices.list().len(), 1);
    assert_eq!(devices.devices.list()[0].device.id.to_string(), relay);
    let again = local(&mut devices, "DELETE", &format!("/api/v1/devices/{th}"), "");
    assert_eq!(error(&again), (404, "Device not found".into()));
}

#[test]
fn other_paths_wrong_methods_big_bodies_and_foreign_hosts_are_refused() {
    let (mut devices, th, _) = two_devices();
    assert_eq!(
        error(&local(&mut devices, "GET", "/api/v1/modes", "")),
        (404, "Not found".into())
    );
    assert_eq!(
        error(&local(&mut devices, "GET", "/elsewhere", "")),
        (404, "Not found".into())
    );
    assert_eq!(
        error(&local(&mut devices, "POST", "/api/v1/devices", "")),
        (405, "Method not allowed".into())
    );
    assert_eq!(
        error(&local(
            &mut devices,
            "PATCH",
            &format!("/api/v1/devices/{th}"),
            ""
        )),
        (405, "Method not allowed".into())
    );
    let big = vec![b' '; MAX_BODY_BYTES + 1];
    let path = format!("/api/v1/devices/{th}");
    assert_eq!(
        error(&call(&mut devices, "PUT", &path, Some(HOST), &big)),
        (413, "Body too large".into())
    );
    for host in [
        Some("evil.example"),
        Some("evil.example:8080"),
        Some("127.0.0.1:9"),
        None,
    ] {
        assert_eq!(
            error(&call(&mut devices, "GET", "/api/v1/devices", host, b"")),
            (403, "Forbidden host".into()),
            "{host:?}"
        );
    }
    // localhost, with the port, is fine; the query string is ignored.
    let ok = call(
        &mut devices,
        "GET",
        "/api/v1/devices?x=1",
        Some("localhost:8080"),
        b"",
    );
    assert_eq!(ok.status, 200);
}

#[test]
fn the_64_character_limit_counts_the_trimmed_name() {
    let (mut devices, th, _) = two_devices();
    let path = format!("/api/v1/devices/{th}");
    let padded = format!(r#"{{"name":"  {}  "}}"#, "a".repeat(64));
    let ok = local(&mut devices, "PUT", &path, &padded);
    assert_eq!(ok.status, 200);
    assert_eq!(json(&ok)["name"], "a".repeat(64));
    let too_long = format!(r#"{{"name":"  {}  "}}"#, "a".repeat(65));
    assert_eq!(
        error(&local(&mut devices, "PUT", &path, &too_long)),
        (400, "Invalid name".into())
    );
}

#[test]
fn a_post_or_put_without_a_json_content_type_is_a_415_whatever_the_route() {
    let (mut home, id, _) = two_devices();
    for (method, path) in [
        ("POST", "/api/v1/zones".to_string()),
        ("PUT", format!("/api/v1/devices/{id}")),
        ("POST", "/api/v1/equipments".to_string()),
    ] {
        for content_type in [Some("text/plain"), None] {
            let response = handle(
                &mut home,
                &Request {
                    method,
                    path: &path,
                    host: Some("127.0.0.1:8080"),
                    port: PORT,
                    reach: Reach::Loopback,
                    content_type,
                    authorization: None,
                    body: br#"{"name":"x"}"#,
                },
                T0 + 2000,
                &mut counter(),
            );
            assert_eq!(
                error(&response),
                (415, "Unsupported Media Type".to_string())
            );
        }
    }
    // Nothing was created, and a GET or DELETE has no body to type.
    assert_eq!(local(&mut home, "GET", "/api/v1/zones", "").status, 200);
}

#[test]
fn every_route_of_the_table_answers_something_other_than_the_route_level_not_found() {
    use solite_core::api::ROUTES;
    // The body of a path the router does not know.
    let unknown = r#"{"error":"Not found"}"#;
    let (mut home, _, _) = two_devices();
    let unknown_id = "11111111-1111-4111-8111-111111111111";
    for (method, pattern) in ROUTES {
        // A parameter is an id that no entity has: the answer is the entity's own 404.
        let path: String = pattern
            .split('/')
            .map(|part| {
                if part.starts_with(':') {
                    unknown_id
                } else {
                    part
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        let response = local(&mut home, method, &path, "{}");
        assert_ne!(
            response.body.as_deref(),
            Some(unknown),
            "{method} {pattern} is served"
        );
        assert_ne!(response.status, 405, "{method} {pattern} is allowed");
    }
    // The check itself sees an unlisted path: the router answers it with the route-level body.
    let response = local(&mut home, "GET", "/api/v1/nothing-here", "");
    assert_eq!(response.body.as_deref(), Some(unknown));
}

#[test]
fn the_routes_of_the_table_are_written_with_the_prefix_and_colon_parameters() {
    use solite_core::api::ROUTES;
    for (method, pattern) in ROUTES {
        assert!(
            ["GET", "POST", "PUT", "DELETE"].contains(method),
            "{method}"
        );
        assert!(pattern.starts_with("/api/v1/"), "{pattern}");
        assert!(
            !pattern.contains('{') && !pattern.contains('?'),
            "{pattern}"
        );
    }
    let mut seen = ROUTES.to_vec();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), ROUTES.len(), "no route twice");
}

// ---- Spec 028, R4: a removed device stays removed until it joins again ----

fn list_len(home: &mut Home) -> usize {
    json(&local(home, "GET", "/api/v1/devices", ""))
        .as_array()
        .unwrap()
        .len()
}

#[test]
fn a_removed_device_that_sends_a_measure_is_not_added_back() {
    use solite_core::radio::measures::{Measure, Value};
    use solite_core::radio::{MeasureEvent, Sender};
    let (mut home, th, _) = two_devices();
    let response = local(&mut home, "DELETE", &format!("/api/v1/devices/{th}"), "");
    assert_eq!(response.status, 204);
    assert_eq!(list_len(&mut home), 1);
    let event = MeasureEvent {
        sender: Sender::Eui64([0, 0x12, 0x4B, 0, 0x12, 0x34, 0x56, 0x78]),
        endpoint: 1,
        measures: vec![Measure {
            kind: "temperature",
            value: Value::Number(20.0),
            unit: "°C",
            device_value: None,
        }],
    };
    home.measures(&event, T0 + 5000, &mut counter()).unwrap();
    assert_eq!(list_len(&mut home), 1, "the measure adds nothing");
    let gone = local(&mut home, "GET", &format!("/api/v1/devices/{th}"), "");
    assert_eq!(error(&gone), (404, "Device not found".to_string()));
}

#[test]
fn a_removed_device_that_announces_itself_outside_a_pairing_window_is_not_added_back() {
    use solite_core::radio::Activity;
    let (mut home, th, _) = two_devices();
    local(&mut home, "DELETE", &format!("/api/v1/devices/{th}"), "");
    let announce = Activity::Announced {
        eui64: [0, 0x12, 0x4B, 0, 0x12, 0x34, 0x56, 0x78],
        capability: 0x80,
    };
    home.devices.apply_activity(&announce, T0 + 5000);
    assert_eq!(list_len(&mut home), 1);
    let gone = local(&mut home, "GET", &format!("/api/v1/devices/{th}"), "");
    assert_eq!(gone.status, 404);
}
