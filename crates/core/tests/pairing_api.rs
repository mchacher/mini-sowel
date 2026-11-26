//! The pairing route (spec 017, R1): `/api/v1/integrations/solite/permit-join`, and the 409 of a
//! device being paired.

use solite_core::api::{Reach, Request, Response, handle};
use solite_core::devices::Devices;
use solite_core::home::Home;

const T0: u64 = 1_795_252_323_120;
const PORT: u16 = 8080;
const ROUTE: &str = "/api/v1/integrations/solite/permit-join";

fn counter() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        buffer.fill(n);
        Ok(())
    }
}

fn call(
    home: &mut Home,
    method: &str,
    path: &str,
    host: &str,
    content_type: &str,
    body: &str,
) -> Response {
    handle(
        home,
        &Request {
            method,
            path,
            host: Some(host),
            port: PORT,
            reach: Reach::Loopback,
            content_type: Some(content_type),
            authorization: None,
            content_length: None,
            body: body.as_bytes(),
        },
        T0,
        &mut counter(),
    )
}

fn local(home: &mut Home, method: &str, path: &str, body: &str) -> Response {
    call(
        home,
        method,
        path,
        "127.0.0.1:8080",
        "application/json",
        body,
    )
}

fn text(response: &Response) -> &str {
    response.body.as_deref().expect("a body")
}

fn error(response: &Response) -> (u16, String) {
    let value: serde_json::Value = serde_json::from_str(text(response)).unwrap();
    (
        response.status,
        value["error"].as_str().unwrap().to_string(),
    )
}

#[test]
fn a_closed_window_reads_closed_with_nothing_remaining() {
    let mut home = Home::new(T0);
    let response = local(&mut home, "GET", ROUTE, "");
    assert_eq!(response.status, 200);
    assert_eq!(text(&response), r#"{"permitJoin":false,"remaining":0}"#);
}

#[test]
fn a_post_answers_202_with_the_state_asked_for_and_records_the_request() {
    let mut home = Home::new(T0);
    let response = local(&mut home, "POST", ROUTE, r#"{"time":60}"#);
    assert_eq!(response.status, 202);
    assert_eq!(text(&response), r#"{"permitJoin":true,"remaining":60}"#);
    assert_eq!(home.pairing_mut().take_request(), Some(60));
    // The engine has not taken a later one yet: a GET agrees with the POST.
    local(&mut home, "POST", ROUTE, r#"{"time":45}"#);
    let response = local(&mut home, "GET", ROUTE, "");
    assert_eq!(text(&response), r#"{"permitJoin":true,"remaining":45}"#);
}

#[test]
fn a_post_with_no_time_asks_180_seconds() {
    let mut home = Home::new(T0);
    let response = local(&mut home, "POST", ROUTE, "{}");
    assert_eq!(response.status, 202);
    assert_eq!(text(&response), r#"{"permitJoin":true,"remaining":180}"#);
}

#[test]
fn time_zero_closes_whether_it_was_open_or_not() {
    let mut home = Home::new(T0);
    let response = local(&mut home, "POST", ROUTE, r#"{"time":0}"#);
    assert_eq!(response.status, 202);
    assert_eq!(text(&response), r#"{"permitJoin":false,"remaining":0}"#);
    assert_eq!(home.pairing_mut().take_request(), Some(0));
    home.pairing_mut().opened(T0 + 60_000);
    let response = local(&mut home, "POST", ROUTE, r#"{"time":0}"#);
    assert_eq!(text(&response), r#"{"permitJoin":false,"remaining":0}"#);
    assert_eq!(home.pairing_mut().take_request(), Some(0));
}

#[test]
fn the_bounds_of_time_are_1_and_254() {
    let mut home = Home::new(T0);
    for time in [1, 254] {
        let response = local(&mut home, "POST", ROUTE, &format!(r#"{{"time":{time}}}"#));
        assert_eq!(response.status, 202);
        assert_eq!(
            text(&response),
            format!(r#"{{"permitJoin":true,"remaining":{time}}}"#)
        );
    }
}

#[test]
fn a_time_that_is_not_an_integer_from_0_to_254_is_refused_and_nothing_is_recorded() {
    let mut home = Home::new(T0);
    for body in [
        r#"{"time":255}"#,
        r#"{"time":-1}"#,
        r#"{"time":1.5}"#,
        r#"{"time":"60"}"#,
        r#"{"time":null}"#,
    ] {
        assert_eq!(
            error(&local(&mut home, "POST", ROUTE, body)),
            (400, "Invalid field: time".to_string()),
            "{body}"
        );
    }
    assert_eq!(home.pairing_mut().take_request(), None);
}

#[test]
fn another_field_or_a_body_that_is_not_an_object_is_refused() {
    let mut home = Home::new(T0);
    assert_eq!(
        error(&local(&mut home, "POST", ROUTE, r#"{"time":60,"x":1}"#)),
        (400, "Unknown field: x".to_string())
    );
    assert_eq!(
        error(&local(&mut home, "POST", ROUTE, "[]")),
        (400, "Invalid JSON".to_string())
    );
}

#[test]
fn another_integration_or_action_is_a_404_and_another_method_a_405() {
    let mut home = Home::new(T0);
    assert_eq!(
        error(&local(
            &mut home,
            "POST",
            "/api/v1/integrations/zigbee2mqtt/permit-join",
            "{}"
        )),
        (404, "Integration not found".to_string())
    );
    assert_eq!(
        error(&local(
            &mut home,
            "GET",
            "/api/v1/integrations/solite/other",
            ""
        )),
        (404, "Not found".to_string())
    );
    for method in ["PUT", "DELETE"] {
        assert_eq!(local(&mut home, method, ROUTE, "{}").status, 405);
    }
}

#[test]
fn the_checks_of_the_api_apply_to_the_route() {
    let mut home = Home::new(T0);
    let response = call(
        &mut home,
        "POST",
        ROUTE,
        "127.0.0.1:8080",
        "text/plain",
        "{}",
    );
    assert_eq!(response.status, 415);
    let response = call(
        &mut home,
        "GET",
        ROUTE,
        "evil.example:8080",
        "application/json",
        "",
    );
    assert_eq!(response.status, 403);
}

#[test]
fn a_refusal_is_read_with_get_and_the_next_post_clears_it() {
    let mut home = Home::new(T0);
    home.pairing_mut()
        .refused("the radio refused permitJoining (status 0x70)");
    let response = local(&mut home, "GET", ROUTE, "");
    assert_eq!(response.status, 200);
    assert_eq!(
        text(&response),
        r#"{"permitJoin":false,"remaining":0,"error":"the radio refused permitJoining (status 0x70)"}"#
    );
    local(&mut home, "POST", ROUTE, r#"{"time":30}"#);
    let response = local(&mut home, "GET", ROUTE, "");
    assert!(!text(&response).contains("error"), "{}", text(&response));
}

#[test]
fn the_remaining_seconds_count_down_on_the_engines_clock() {
    let mut home = Home::new(T0);
    home.pairing_mut().set_clock(std::sync::Arc::new(|| 10_200));
    home.pairing_mut().opened(60_000);
    let response = local(&mut home, "GET", ROUTE, "");
    assert_eq!(text(&response), r#"{"permitJoin":true,"remaining":50}"#);
}

#[test]
fn a_device_being_paired_cannot_be_deleted_until_its_setup_is_done() {
    let mut devices = Devices::new();
    let eui = [0, 0x12, 0x4B, 0, 0x12, 0x34, 0x56, 0x7A];
    devices
        .identified(eui, Some("SONOFF"), Some("ZBMINIR2"), T0, &mut counter())
        .unwrap();
    let id = devices.list()[0].device.id.to_string();
    let mut home = Home::from_devices(devices, T0);
    home.pairing_mut().set_busy(&[eui]);
    let path = format!("/api/v1/devices/{id}");
    assert_eq!(
        error(&local(&mut home, "DELETE", &path, "")),
        (409, "Device is being paired".to_string())
    );
    home.pairing_mut().set_busy(&[]);
    assert_eq!(local(&mut home, "DELETE", &path, "").status, 204);
}
