//! Who may call over the local network (spec 022 R4): the `Host` rule of `Reach::Lan`, the pairing
//! route answering there as on the Mac, and the answers being the same as under `Reach::Loopback`.

use serde_json::{Value, json};
use solite_core::api::{MAX_BODY_BYTES, Reach, Request, Response, handle};
use solite_core::devices::Devices;
use solite_core::home::Home;

const T0: u64 = 1_795_252_323_120;
const NAME: &str = "solite-a1b2c3";
const ROOT: &str = "00000000-0000-0000-0000-000000000001";
const PAIRING: &str = "/api/v1/integrations/solite/permit-join";

fn counter() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        buffer.fill(n);
        Ok(())
    }
}

/// A SNZB-02 and a relay (one data, one order).
fn home() -> Home {
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
    Home::from_devices(devices, T0)
}

fn call(
    home: &mut Home,
    reach: Reach,
    port: u16,
    host: Option<&str>,
    method: &str,
    path: &str,
    body: &[u8],
) -> Response {
    handle(
        home,
        &Request {
            method,
            path,
            host,
            port,
            reach,
            content_type: Some("application/json"),
            authorization: None,
            content_length: None,
            body,
        },
        T0 + 2000,
        &mut counter(),
    )
}

/// A call from the home's network: the box's name is `NAME`, the server listens on port 80.
fn lan(home: &mut Home, host: Option<&str>, method: &str, path: &str) -> Response {
    call(home, Reach::Lan { name: NAME }, 80, host, method, path, b"")
}

#[test]
fn private_addresses_localhost_and_the_box_name_are_allowed_on_the_lan() {
    let mut home = home();
    for host in [
        "192.168.1.20",
        "192.168.1.20:80",
        "10.1.2.3",
        "172.16.0.5",
        "172.31.255.1",
        "127.0.0.1:80",
        "localhost",
        "LOCALHOST:80",
    ] {
        let response = lan(&mut home, Some(host), "GET", "/api/v1/devices");
        assert_eq!(response.status, 200, "{host}");
    }
}

#[test]
fn the_box_name_with_a_home_suffix_is_allowed_whatever_its_case() {
    let mut home = home();
    for host in [
        "solite-a1b2c3",
        "SOLITE-A1B2C3.local",
        "solite-a1b2c3.lan",
        "solite-a1b2c3.home",
        "solite-a1b2c3.home.arpa",
        "solite-a1b2c3.local:80",
    ] {
        let response = lan(&mut home, Some(host), "GET", "/api/v1/devices");
        assert_eq!(response.status, 200, "{host}");
    }
}

#[test]
fn public_addresses_ipv6_other_names_other_ports_and_no_host_are_refused_on_the_lan() {
    let mut home = home();
    for host in [
        Some("8.8.8.8"),
        Some("172.32.0.1"),
        Some("172.15.255.1"),
        Some("192.169.0.1"),
        Some("[fe80::1]"),
        Some("evil.example"),
        Some("solite-a1b2c3.evil.example"),
        Some("solite-ffffff"),
        Some("192.168.1.20:8080"),
        Some("192.168.1.20:"),
        Some("192.168.1"),
        Some("192.168.1.20.5"),
        None,
    ] {
        let response = lan(&mut home, host, "GET", "/api/v1/devices");
        assert_eq!(response.status, 403, "{host:?}");
        assert_eq!(
            response.body.as_deref(),
            Some(r#"{"error":"Forbidden host"}"#),
            "{host:?}"
        );
    }
}

#[test]
fn the_loopback_rule_is_unchanged() {
    let mut home = home();
    let loopback = |home: &mut Home, host| {
        call(
            home,
            Reach::Loopback,
            8080,
            Some(host),
            "GET",
            "/api/v1/devices",
            b"",
        )
        .status
    };
    assert_eq!(loopback(&mut home, "127.0.0.1:8080"), 200);
    assert_eq!(loopback(&mut home, "localhost:8080"), 200);
    for host in ["192.168.1.20:8080", "127.0.0.1", "127.0.0.1:80", NAME] {
        assert_eq!(loopback(&mut home, host), 403, "{host}");
    }
}

#[test]
fn the_pairing_route_answers_on_the_lan_as_on_the_mac() {
    // Spec 029, D49: the 404 of D28 is gone; the token is the gate's (see `tests/auth.rs`).
    let mut home = home();
    let lan = |home: &mut Home, method| {
        call(
            home,
            Reach::Lan { name: NAME },
            80,
            Some("192.168.1.20"),
            method,
            PAIRING,
            br#"{"time":30}"#,
        )
    };
    let get = lan(&mut home, "GET");
    assert_eq!(get.status, 200);
    assert_eq!(
        get.body.as_deref(),
        Some(r#"{"permitJoin":false,"remaining":0}"#)
    );
    let post = lan(&mut home, "POST");
    assert_eq!(post.status, 202);
    assert_eq!(
        post.body.as_deref(),
        Some(r#"{"permitJoin":true,"remaining":30}"#)
    );
}

#[test]
fn the_pairing_route_answers_under_loopback_as_before() {
    let mut home = home();
    let loopback = |home: &mut Home, method| {
        call(
            home,
            Reach::Loopback,
            8080,
            Some("127.0.0.1:8080"),
            method,
            PAIRING,
            br#"{"time":30}"#,
        )
        .status
    };
    assert_eq!(loopback(&mut home, "GET"), 200);
    assert_eq!(loopback(&mut home, "POST"), 202);
}

#[test]
fn a_body_of_16384_bytes_is_parsed_and_one_of_16385_is_413_on_the_lan() {
    let mut home = home();
    let post = |home: &mut Home, size: usize| {
        call(
            home,
            Reach::Lan { name: NAME },
            80,
            Some("192.168.1.20"),
            "POST",
            "/api/v1/zones",
            &vec![b' '; size],
        )
    };
    let at_bound = post(&mut home, MAX_BODY_BYTES);
    assert_ne!(at_bound.status, 413, "a body at the bound is parsed");
    let over = post(&mut home, MAX_BODY_BYTES + 1);
    assert_eq!(over.status, 413);
    assert_eq!(over.body.as_deref(), Some(r#"{"error":"Body too large"}"#));
}

/// The same requests, one after another, to a fresh home: (status, body) of each.
fn sequence(reach: Reach, port: u16, host: &str) -> Vec<(u16, Option<String>)> {
    let mut home = home();
    let mut answers = Vec::new();
    let mut send = |home: &mut Home, method: &str, path: &str, body: Value| -> Value {
        let response = call(
            home,
            reach,
            port,
            Some(host),
            method,
            path,
            body.to_string().as_bytes(),
        );
        answers.push((response.status, response.body.clone()));
        response
            .body
            .and_then(|b| serde_json::from_str(&b).ok())
            .unwrap_or(Value::Null)
    };
    let devices = send(&mut home, "GET", "/api/v1/devices", json!({}));
    let relay = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["data"].as_array().is_some_and(|d| d.len() == 1))
        .unwrap()
        .clone();
    let zone = send(
        &mut home,
        "POST",
        "/api/v1/zones",
        json!({"name": "Garage", "parentId": ROOT}),
    );
    let zone_id = zone["id"].as_str().unwrap().to_string();
    let equipment = send(
        &mut home,
        "POST",
        "/api/v1/equipments",
        json!({"name": "Light", "type": "light_onoff", "zoneId": zone_id, "deviceIds": [relay["id"]]}),
    );
    let equipment_id = equipment["id"].as_str().unwrap().to_string();
    send(&mut home, "GET", "/api/v1/zones", json!({}));
    send(&mut home, "GET", "/api/v1/zones/aggregation", json!({}));
    send(&mut home, "GET", "/api/v1/equipments", json!({}));
    send(
        &mut home,
        "GET",
        &format!("/api/v1/equipments/{equipment_id}"),
        json!({}),
    );
    send(
        &mut home,
        "PUT",
        &format!("/api/v1/devices/{}", relay["id"].as_str().unwrap()),
        json!({"name": "Relay"}),
    );
    send(&mut home, "GET", "/api/v1/recipes", json!({}));
    send(&mut home, "GET", "/api/v1/recipe-instances", json!({}));
    send(&mut home, "GET", "/api/v1/devices/nope", json!({}));
    send(
        &mut home,
        "DELETE",
        &format!("/api/v1/equipments/{equipment_id}"),
        json!({}),
    );
    answers
}

#[test]
fn the_same_requests_give_the_same_bodies_and_statuses_under_loopback_and_lan() {
    let loopback = sequence(Reach::Loopback, 8080, "127.0.0.1:8080");
    let on_lan = sequence(Reach::Lan { name: NAME }, 80, "192.168.1.20");
    assert!(loopback.len() >= 12);
    assert!(loopback.iter().all(|(status, _)| *status < 500));
    assert_eq!(loopback, on_lan);
}

#[test]
fn the_17_requests_of_the_largest_creation_are_all_201_on_the_lan() {
    let mut home = home();
    let send = |home: &mut Home, method: &str, path: &str, body: Value| -> (u16, Value) {
        let response = call(
            home,
            Reach::Lan { name: NAME },
            80,
            Some("192.168.1.20"),
            method,
            path,
            body.to_string().as_bytes(),
        );
        let value = response
            .body
            .and_then(|b| serde_json::from_str(&b).ok())
            .unwrap_or(Value::Null);
        (response.status, value)
    };
    let (_, devices) = send(&mut home, "GET", "/api/v1/devices", json!({}));
    let relay = devices
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["orders"].as_array().is_some_and(|o| !o.is_empty()))
        .unwrap()
        .clone();
    let data = relay["data"][0]["id"].as_str().unwrap().to_string();
    let order = relay["orders"][0]["id"].as_str().unwrap().to_string();
    let (status, zone) = send(
        &mut home,
        "POST",
        "/api/v1/zones",
        json!({"name": "Measure 33", "parentId": ROOT}),
    );
    assert_eq!(status, 201, "{zone}");
    let (status, equipment) = send(
        &mut home,
        "POST",
        "/api/v1/equipments",
        json!({"name": "Measure 33", "type": "switch", "zoneId": zone["id"]}),
    );
    assert_eq!(status, 201, "{equipment}");
    let id = equipment["id"].as_str().unwrap().to_string();
    let mut created = 1;
    for i in 1..=8 {
        let (status, body) = send(
            &mut home,
            "POST",
            &format!("/api/v1/equipments/{id}/data-bindings"),
            json!({"deviceDataId": data, "alias": format!("m{i}")}),
        );
        assert_eq!(status, 201, "data {i}: {body}");
        created += 1;
    }
    for i in 1..=8 {
        let (status, body) = send(
            &mut home,
            "POST",
            &format!("/api/v1/equipments/{id}/order-bindings"),
            json!({"deviceOrderId": order, "alias": format!("o{i}")}),
        );
        assert_eq!(status, 201, "order {i}: {body}");
        created += 1;
    }
    assert_eq!(created, 17);
    let (_, equipment) = send(
        &mut home,
        "GET",
        &format!("/api/v1/equipments/{id}"),
        json!({}),
    );
    assert_eq!(equipment["dataBindings"].as_array().unwrap().len(), 8);
    assert_eq!(equipment["orderBindings"].as_array().unwrap().len(), 8);
}
