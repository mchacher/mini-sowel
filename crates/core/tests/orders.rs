//! Orders (spec 011): the route's validation and the values Sowel's rules give, the commands per
//! device, the queue and the runner against the simulated radio, with fake time.

mod support;

use mini_sowel_core::api::{Reply, Request, handle_stream};
use mini_sowel_core::home::Home;
use mini_sowel_core::orders::{
    DEADLINE_MS, DeviceCommand, OrderQueue, OrderRequest, OrderRunner, commands_for,
};
use mini_sowel_core::radio::zcl::Command;
use mini_sowel_core::radio::{NetworkSettings, Radio};
use mini_sowel_sim::{Harness, Kind, Ncp, SentMessage, SimDevice, harness};
use serde_json::{Value, json};
use support::{HOST, PORT, Rig, T0, eui};

/// What an order gives: the commands queued, or the status and text of the refusal.
fn commands(
    rig: &mut Rig,
    equipment: &str,
    alias: &str,
    body: &str,
) -> Result<Vec<Command>, (u16, String)> {
    let path = format!("/api/v1/equipments/{equipment}/orders/{alias}");
    let reply = handle_stream(
        &mut rig.home,
        &Request {
            method: "POST",
            path: &path,
            host: Some(HOST),
            port: PORT,
            content_type: Some("application/json"),
            body: body.as_bytes(),
        },
        rig.now,
        &mut *rig.random,
    );
    match reply {
        Reply::Order(_wait) => {
            let queued = rig
                .home
                .orders()
                .take(rig.now)
                .expect("the order is queued");
            rig.home.orders().done();
            Ok(queued.request.commands.iter().map(|c| c.command).collect())
        }
        Reply::Whole(response) => {
            let body: Value = serde_json::from_str(&response.body.unwrap_or_default()).unwrap();
            Err((
                response.status,
                body["error"].as_str().unwrap_or("").to_string(),
            ))
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// A home with one relay in an equipment: the equipment's id.
fn light() -> (Rig, String) {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let relay = rig.relay(0x7A);
    let id = rig.equipment("Light", "light_onoff", &zone, &[&relay]);
    (rig, id)
}

#[test]
fn the_state_values_follow_sowels_rules_and_anything_else_is_a_400() {
    let (mut rig, id) = light();
    let on = Ok(vec![Command::On]);
    let off = Ok(vec![Command::Off]);
    let toggle = Ok(vec![Command::Toggle]);
    for body in [
        r#"{"value":true}"#,
        r#"{"value":"ON"}"#,
        r#"{"value":" On "}"#,
        r#"{"value":"true"}"#,
        r#"{"value":"1"}"#,
        r#"{"value":1}"#,
        r#"{"value":""}"#,
        r#"{"value":null}"#,
        r#"{}"#,
        "",
        r#"{"other":3}"#,
    ] {
        assert_eq!(commands(&mut rig, &id, "state", body), on, "{body:?}");
    }
    for body in [
        r#"{"value":false}"#,
        r#"{"value":"off"}"#,
        r#"{"value":"0"}"#,
        r#"{"value":0}"#,
        r#"{"value":"FALSE"}"#,
    ] {
        assert_eq!(commands(&mut rig, &id, "state", body), off, "{body:?}");
    }
    assert_eq!(
        commands(&mut rig, &id, "state", r#"{"value":"Toggle"}"#),
        toggle
    );
    for body in [
        r#"{"value":3}"#,
        r#"{"value":"x"}"#,
        r#"{"value":[true]}"#,
        r#"{"value":1.5}"#,
    ] {
        assert_eq!(
            commands(&mut rig, &id, "state", body),
            Err((400, "Invalid value for state".to_string())),
            "{body:?}"
        );
    }
    // A body that is not a JSON object is refused as everywhere else.
    assert_eq!(
        commands(&mut rig, &id, "state", "[1]"),
        Err((400, "Invalid JSON".to_string()))
    );
}

/// A dimmable light: the relay's state and the brightness (clusters 0x0006 and 0x0008).
fn dimmer(rig: &mut Rig, last: u8) -> String {
    let e = eui(last);
    rig.home.devices.announced(e, 0x8C, rig.now);
    rig.home
        .devices
        .identified(e, Some("TEST"), Some("DIM01"), rig.now, &mut *rig.random)
        .unwrap();
    rig.home
        .devices
        .reporting_configured(e, &[0x0006, 0x0008], rig.now, &mut *rig.random)
        .unwrap();
    rig.device(last)
}

#[test]
fn the_brightness_takes_a_whole_number_from_0_to_255_and_255_counts_as_254() {
    let mut rig = Rig::new();
    let zone = rig.zone("Salon");
    let device = dimmer(&mut rig, 0x7B);
    let id = rig.equipment("Lamp", "light_dimmable", &zone, &[&device]);
    for (body, level) in [
        (r#"{"value":0}"#, 0),
        (r#"{"value":254}"#, 254),
        (r#"{"value":255}"#, 254),
        (r#"{"value":"200"}"#, 200),
        (r#"{"value":200.0}"#, 200),
    ] {
        assert_eq!(
            commands(&mut rig, &id, "brightness", body),
            Ok(vec![Command::MoveToLevel(level)]),
            "{body:?}"
        );
    }
    for body in [
        r#"{"value":12.5}"#,
        r#"{"value":-1}"#,
        r#"{"value":256}"#,
        r#"{"value":"a"}"#,
        r#"{"value":true}"#,
        r#"{}"#,
    ] {
        assert_eq!(
            commands(&mut rig, &id, "brightness", body),
            Err((400, "Invalid value for brightness".to_string())),
            "{body:?}"
        );
    }
}

#[test]
fn an_unknown_equipment_a_disabled_one_and_an_unknown_alias_are_refused_with_sowels_texts() {
    let (mut rig, id) = light();
    let unknown = "11111111-2222-4333-8444-555555555555";
    assert_eq!(
        commands(&mut rig, unknown, "state", r#"{"value":true}"#),
        Err((404, "Equipment not found".to_string()))
    );
    assert_eq!(
        commands(&mut rig, &id, "nothing", r#"{"value":true}"#),
        Err((404, "Order alias not found: nothing".to_string()))
    );
    let (status, _) = rig.put(
        &format!("/api/v1/equipments/{id}"),
        json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    assert_eq!(
        commands(&mut rig, &id, "state", r#"{"value":true}"#),
        Err((400, "Equipment is disabled".to_string()))
    );
    // A route that takes POST only.
    let (status, _) = rig.get(&format!("/api/v1/equipments/{id}/orders/state"));
    assert_eq!(status, 405);
}

#[test]
fn two_devices_under_one_alias_give_two_commands_each_with_its_own_address_and_endpoint() {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let (first, second) = (rig.relay(0x7A), rig.relay(0x7B));
    let one = rig.equipment("One", "light_onoff", &zone, &[&first]);
    let two = rig.equipment("Two", "light_onoff", &zone, &[&second]);
    // The API binds one order per alias in an equipment: the two bindings come from two.
    let binding = |id: &str| rig.home.equipments().get(id).unwrap().order_bindings[0].clone();
    let bindings = [binding(&one), binding(&two)];
    // The registry knows the first one's route; the second's is not known yet.
    rig.home
        .devices
        .set_route(&mini_sowel_core::radio::RouteUpdate {
            eui64: eui(0x7A),
            nwk: 0x1234,
            endpoint: Some(3),
        });
    let list = commands_for(&rig.home.devices, &bindings, "state", &json!("off")).unwrap();
    assert_eq!(
        list,
        vec![
            DeviceCommand {
                eui64: eui(0x7A),
                nwk: Some(0x1234),
                endpoint: 3,
                command: Command::Off
            },
            DeviceCommand {
                eui64: eui(0x7B),
                nwk: None,
                endpoint: 1,
                command: Command::Off
            },
        ]
    );
}

// ---- The queue and the runner, against the simulated radio ----

fn scripted() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    let mut n = 0u8;
    move |buffer: &mut [u8]| {
        n = n.wrapping_add(1);
        buffer.fill(n);
        // Valid extended PAN and PAN.
        buffer[0] = 1;
        Ok(())
    }
}

fn relay(last: u8, nwk: u16) -> SimDevice {
    let mut wire = eui(last);
    wire.reverse();
    let mut device = SimDevice::of_kind(Kind::Relay, wire, None, None);
    device.nwk = nwk;
    device.auto_join = true;
    device
}

/// A radio with these relays paired (each one's route is known to the radio).
fn paired(devices: Vec<SimDevice>) -> Harness {
    let mut ncp = Ncp::recording();
    ncp.devices = devices;
    let mut h = harness(ncp);
    h.radio.identify().unwrap();
    h.radio
        .start_network(
            &NetworkSettings::default(),
            &mut scripted(),
            &mut |_| Ok(()),
        )
        .unwrap();
    h.radio.permit_join(180).unwrap();
    let end = h.clock.get() + 60_000;
    loop {
        h.radio.poll_pairing().unwrap();
        if !h.radio.pairing_busy() || h.clock.get() >= end {
            break;
        }
    }
    h
}

fn request(commands: Vec<DeviceCommand>) -> OrderRequest {
    OrderRequest {
        equipment: mini_sowel_core::devices::Id::ROOT_ZONE,
        alias: "state".to_string(),
        commands,
    }
}

fn to(last: u8, nwk: Option<u16>, command: Command) -> DeviceCommand {
    DeviceCommand {
        eui64: eui(last),
        nwk,
        endpoint: 1,
        command,
    }
}

/// Polls the radio and steps the runner until `until` is true or `ms` of fake time passed.
fn run<T: mini_sowel_core::radio::RadioTransport, C: FnMut() -> u64>(
    radio: &mut Radio<T, C>,
    runner: &mut OrderRunner,
    queue: &OrderQueue,
    clock: &std::cell::Cell<u64>,
    ms: u64,
) {
    let end = clock.get() + ms;
    while clock.get() < end {
        radio.poll_pairing().unwrap();
        runner.step(radio, queue, T0 + clock.get());
    }
}

fn command_sends(h: &Harness) -> Vec<SentMessage> {
    h.ncp
        .borrow()
        .sent
        .iter()
        .filter(|m| m.profile == 0x0104 && m.cluster == 0x0006 && m.message[0] & 0x03 == 0x01)
        .cloned()
        .collect()
}

#[test]
fn an_order_pushed_by_the_core_without_a_waiter_is_executed_through_the_same_queue() {
    let mut h = paired(vec![relay(0x7A, 0x2001)]);
    let home = Home::new(T0);
    let queue = home.orders().clone();
    let mut runner = OrderRunner::new();
    let now = T0 + h.clock.get();
    // The core's own order: no HTTP, no reply slot.
    queue
        .push_unattended(request(vec![to(0x7A, None, Command::On)]), now)
        .unwrap();
    assert_eq!(queue.len(), 1);
    run(&mut h.radio, &mut runner, &queue, &h.clock, 500);
    let sent = command_sends(&h);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].message[2], 0x01, "On");
    assert_eq!(
        sent[0].destination, 0x2001,
        "the address came from the radio's own cache"
    );
    assert!(queue.is_empty());
    assert!(!runner.busy());
}

#[test]
fn an_order_goes_to_two_devices_in_parallel_and_succeeds_when_one_answers() {
    let mut h = paired(vec![relay(0x7A, 0x2001), relay(0x7B, 0x2002)]);
    // The second relay never answers.
    h.ncp.borrow_mut().devices[1].silent = true;
    let home = Home::new(T0);
    let queue = home.orders().clone();
    let mut runner = OrderRunner::new();
    let wait = queue
        .push_waited(
            request(vec![
                to(0x7A, None, Command::On),
                to(0x7B, None, Command::On),
            ]),
            T0 + h.clock.get(),
        )
        .unwrap();
    run(&mut h.radio, &mut runner, &queue, &h.clock, 13_000);
    assert_eq!(wait.wait(std::time::Duration::from_millis(1)), Ok(()));
    // Both were sent at once; the silent one again after 2 s.
    let sent = command_sends(&h);
    assert_eq!(sent.iter().filter(|m| m.destination == 0x2001).count(), 1);
    assert_eq!(sent.iter().filter(|m| m.destination == 0x2002).count(), 2);
}

#[test]
fn nobody_answering_is_a_502_text_after_one_retry_and_the_last_failure_wins() {
    let mut h = paired(vec![relay(0x7A, 0x2001), relay(0x7B, 0x2002)]);
    h.ncp.borrow_mut().devices[0].silent = true;
    // The second one refuses with an error status: not retried.
    h.ncp.borrow_mut().devices[1].refuse_commands = Some(0x81);
    let home = Home::new(T0);
    let queue = home.orders().clone();
    let mut runner = OrderRunner::new();
    let wait = queue
        .push_waited(
            request(vec![
                to(0x7A, None, Command::On),
                to(0x7B, None, Command::On),
            ]),
            T0 + h.clock.get(),
        )
        .unwrap();
    run(&mut h.radio, &mut runner, &queue, &h.clock, 13_000);
    assert_eq!(
        wait.wait(std::time::Duration::from_millis(1)),
        Err("device refused the command (status 0x81)".to_string())
    );
    // Reversed: the silent device is the last one, so its reason is the answer.
    let mut h = paired(vec![relay(0x7A, 0x2001), relay(0x7B, 0x2002)]);
    h.ncp.borrow_mut().devices[1].silent = true;
    h.ncp.borrow_mut().devices[0].refuse_commands = Some(0x81);
    let mut runner = OrderRunner::new();
    let wait = queue
        .push_waited(
            request(vec![
                to(0x7A, None, Command::On),
                to(0x7B, None, Command::On),
            ]),
            T0 + h.clock.get(),
        )
        .unwrap();
    run(&mut h.radio, &mut runner, &queue, &h.clock, 13_000);
    assert_eq!(
        wait.wait(std::time::Duration::from_millis(1)),
        Err("device did not answer".to_string())
    );
}

#[test]
fn a_third_order_is_busy_and_an_order_past_its_deadline_or_without_a_waiter_is_never_sent() {
    let mut h = paired(vec![relay(0x7A, 0x2001)]);
    let home = Home::new(T0);
    let queue = home.orders().clone();
    let mut runner = OrderRunner::new();
    let now = T0 + h.clock.get();
    let on = || request(vec![to(0x7A, None, Command::On)]);

    // At most 2 orders in the system, waiting or being sent.
    let _first = queue.push_waited(on(), now).unwrap();
    let _second = queue.push_waited(on(), now).unwrap();
    assert!(queue.push_waited(on(), now).is_err());
    assert!(queue.push_unattended(on(), now).is_err());
    // The first is taken: it counts until it is done.
    runner.step(&mut h.radio, &queue, now);
    assert!(runner.busy());
    assert!(queue.push_waited(on(), now).is_err());
    run(&mut h.radio, &mut runner, &queue, &h.clock, 1_000);
    assert!(queue.is_empty());
    let sent = command_sends(&h).len();
    assert_eq!(sent, 2, "both orders were sent, one after the other");

    // An order whose deadline passed before it was taken is dropped, never sent; its waiter is
    // told the device did not answer.
    let late = queue.push_waited(on(), T0 + h.clock.get()).unwrap();
    assert!(queue.take(T0 + h.clock.get() + DEADLINE_MS).is_none());
    let late = {
        drop(late);
        queue.push_waited(on(), T0 + h.clock.get()).unwrap()
    };
    h.clock.set(h.clock.get() + DEADLINE_MS + 1);
    run(&mut h.radio, &mut runner, &queue, &h.clock, 200);
    assert_eq!(command_sends(&h).len(), sent);
    assert_eq!(
        late.wait(std::time::Duration::from_millis(1)),
        Err("device did not answer".to_string())
    );
    assert!(queue.is_empty());

    // An order whose waiter is gone is dropped without being sent.
    let gone = queue.push_waited(on(), T0 + h.clock.get()).unwrap();
    drop(gone);
    run(&mut h.radio, &mut runner, &queue, &h.clock, 500);
    assert_eq!(command_sends(&h).len(), sent);
    assert!(queue.is_empty());
}

#[test]
fn the_route_of_a_device_is_kept_in_devices_json_beside_sowels_fields_and_not_in_the_api() {
    use mini_sowel_core::devices::{Devices, parse_list};
    use mini_sowel_core::radio::RouteUpdate;
    let mut rig = Rig::new();
    let relay = rig.relay(0x7A);
    // Unknown: nothing written, nothing in the device API.
    assert!(!rig.home.devices.to_json().unwrap().contains("\"nwk\""));
    rig.home.devices.clear_changes();
    rig.home.devices.set_route(&RouteUpdate {
        eui64: eui(0x7A),
        nwk: 0x2001,
        endpoint: Some(2),
    });
    // A new address is a change of definitions: it is written at once.
    assert_eq!(rig.home.devices.changes(), (true, false));
    let stored = rig.home.devices.to_json().unwrap();
    assert!(
        stored.contains("\"nwk\":8193") && stored.contains("\"endpoint\":2"),
        "{stored}"
    );
    // The cursor that writes the file chunk by chunk writes the same.
    let item = rig.home.devices.get(&relay).unwrap();
    let chunk = serde_json::to_string(&mini_sowel_core::devices::stored_item(item)).unwrap();
    assert!(
        chunk.contains("\"nwk\":8193") && chunk.contains("\"endpoint\":2"),
        "{chunk}"
    );
    let back = Devices::from_list(parse_list(&stored).unwrap());
    let route = back.get(&relay).unwrap().route;
    assert_eq!((route.nwk, route.endpoint), (Some(0x2001), Some(2)));
    // A message that tells the same again changes nothing; one that tells no endpoint keeps it.
    rig.home.devices.clear_changes();
    rig.home.devices.set_route(&RouteUpdate {
        eui64: eui(0x7A),
        nwk: 0x2001,
        endpoint: None,
    });
    assert_eq!(rig.home.devices.changes(), (false, false));
    // A file written before spec 011 has no route.
    let old = stored.replace(",\"nwk\":8193,\"endpoint\":2", "");
    assert!(parse_list(&old).unwrap()[0].route.nwk.is_none());
    // The device API never shows it.
    let (status, body) = rig.get(&format!("/api/v1/devices/{relay}"));
    assert_eq!(status, 200);
    assert!(body.get("nwk").is_none() && body.get("endpoint").is_none());
}

#[test]
fn a_third_order_over_the_route_is_a_503_busy_and_the_queue_frees_as_orders_are_done() {
    let (mut rig, id) = light();
    let path = format!("/api/v1/equipments/{id}/orders/state");
    let ask = |rig: &mut Rig| {
        handle_stream(
            &mut rig.home,
            &Request {
                method: "POST",
                path: &path,
                host: Some(HOST),
                port: PORT,
                content_type: Some("application/json"),
                body: br#"{"value":true}"#,
            },
            rig.now,
            &mut *rig.random,
        )
    };
    let waits: Vec<Reply> = (0..2).map(|_| ask(&mut rig)).collect();
    assert!(waits.iter().all(|r| matches!(r, Reply::Order(_))));
    let Reply::Whole(response) = ask(&mut rig) else {
        panic!("a third order is refused");
    };
    assert_eq!(response.status, 503);
    assert_eq!(response.body.as_deref(), Some(r#"{"error":"Busy"}"#));
    // One taken and done: there is room again.
    assert!(rig.home.orders().take(rig.now).is_some());
    rig.home.orders().done();
    assert!(matches!(ask(&mut rig), Reply::Order(_)));
}

// ---- Late answers, the end of the deadline, radio failures ----

/// A Default Response to the command whose frame was sent, as the device would send it.
fn default_response(sent: &SentMessage, status: u8) -> Vec<u8> {
    vec![0x18, sent.message[1], 0x0B, sent.message[2], status]
}

/// Runs by steps of 100 ms until `done` or `ms` passed.
fn run_until(
    h: &mut Harness,
    runner: &mut OrderRunner,
    queue: &OrderQueue,
    ms: u64,
    mut done: impl FnMut(&Harness) -> bool,
) {
    let end = h.clock.get() + ms;
    while h.clock.get() < end && !done(h) {
        h.radio.poll_pairing().unwrap();
        runner.step(&mut h.radio, queue, T0 + h.clock.get());
    }
}

fn silent_relay_with_an_order() -> (
    Harness,
    OrderRunner,
    OrderQueue,
    mini_sowel_core::orders::OrderWait,
) {
    let h = paired(vec![relay(0x7A, 0x2001)]);
    h.ncp.borrow_mut().devices[0].silent = true;
    let home = Home::new(T0);
    let queue = home.orders().clone();
    let wait = queue
        .push_waited(
            request(vec![to(0x7A, None, Command::On)]),
            T0 + h.clock.get(),
        )
        .unwrap();
    (h, OrderRunner::new(), queue, wait)
}

#[test]
fn an_answer_to_the_first_attempt_that_comes_during_the_pause_is_a_success_and_nothing_is_sent_again()
 {
    let (mut h, mut runner, queue, wait) = silent_relay_with_an_order();
    let start = h.clock.get();
    // 5 s without an answer, then the 2 s pause: 6 s in, the retry is not sent yet.
    run_until(&mut h, &mut runner, &queue, 6_000, |_| false);
    assert!(h.clock.get() >= start + 6_000);
    let sent = command_sends(&h);
    assert_eq!(sent.len(), 1);
    h.ncp
        .borrow_mut()
        .inject(0x2001, 0x0104, 0x0006, 1, &default_response(&sent[0], 0));
    run_until(&mut h, &mut runner, &queue, 8_000, |_| false);
    assert_eq!(wait.wait(std::time::Duration::from_millis(1)), Ok(()));
    assert_eq!(command_sends(&h).len(), 1, "no retry after the answer");
}

#[test]
fn an_answer_to_the_first_attempt_that_comes_during_the_second_is_a_success() {
    let (mut h, mut runner, queue, wait) = silent_relay_with_an_order();
    run_until(&mut h, &mut runner, &queue, 12_000, |h| {
        command_sends(h).len() == 2
    });
    let sent = command_sends(&h);
    assert_eq!(sent.len(), 2);
    assert_ne!(
        sent[0].message[1], sent[1].message[1],
        "one TSN per attempt"
    );
    h.ncp
        .borrow_mut()
        .inject(0x2001, 0x0104, 0x0006, 1, &default_response(&sent[0], 0));
    run_until(&mut h, &mut runner, &queue, 8_000, |_| false);
    assert_eq!(wait.wait(std::time::Duration::from_millis(1)), Ok(()));
    assert_eq!(command_sends(&h).len(), 2);
}

#[test]
fn an_order_taken_with_less_than_an_answer_wait_before_its_deadline_is_not_sent() {
    let mut h = paired(vec![relay(0x7A, 0x2001)]);
    let home = Home::new(T0);
    let queue = home.orders().clone();
    let mut runner = OrderRunner::new();
    let wait = queue
        .push_waited(
            request(vec![to(0x7A, None, Command::On)]),
            T0 + h.clock.get(),
        )
        .unwrap();
    // The runner takes it with 0.3 s left.
    let late = T0 + h.clock.get() + DEADLINE_MS - 300;
    for _ in 0..10 {
        h.radio.poll_pairing().unwrap();
        runner.step(&mut h.radio, &queue, late);
    }
    assert_eq!(command_sends(&h).len(), 0);
    assert_eq!(
        wait.wait(std::time::Duration::from_millis(1)),
        Err("device did not answer".to_string())
    );
    assert!(queue.is_empty());
}

#[test]
fn a_radio_that_fails_while_sending_fails_that_command_with_a_reason_and_polling_goes_on() {
    let mut h = paired(vec![relay(0x7A, 0x2001)]);
    let home = Home::new(T0);
    let queue = home.orders().clone();
    let mut runner = OrderRunner::new();
    let wait = queue
        .push_waited(
            request(vec![to(0x7A, None, Command::On)]),
            T0 + h.clock.get(),
        )
        .unwrap();
    // The radio takes the frames but no longer answers its commands.
    h.ncp.borrow_mut().mode = mini_sowel_sim::Mode::DeafToCommands;
    run_until(&mut h, &mut runner, &queue, 2_000, |_| false);
    let Err(reason) = wait.wait(std::time::Duration::from_millis(1)) else {
        panic!("the order cannot succeed");
    };
    assert!(reason.starts_with("radio error: "), "{reason}");
    assert!(queue.is_empty());
}

fn post_order_typed(
    rig: &mut Rig,
    path: &str,
    content_type: Option<&str>,
    streaming: bool,
) -> Reply {
    let request = Request {
        method: "POST",
        path,
        host: Some(HOST),
        port: PORT,
        content_type,
        body: br#"{"value":true}"#,
    };
    if streaming {
        handle_stream(&mut rig.home, &request, rig.now, &mut *rig.random)
    } else {
        Reply::Whole(mini_sowel_core::api::handle(
            &mut rig.home,
            &request,
            rig.now,
            &mut *rig.random,
        ))
    }
}

#[test]
fn the_plain_handler_answers_501_to_an_order_and_queues_nothing() {
    let (mut rig, id) = light();
    let path = format!("/api/v1/equipments/{id}/orders/state");
    let Reply::Whole(response) = post_order_typed(&mut rig, &path, Some("application/json"), false)
    else {
        panic!("whole");
    };
    assert_eq!(response.status, 501);
    assert_eq!(
        response.body.as_deref(),
        Some(r#"{"error":"Orders need the streaming handler"}"#)
    );
    assert!(rig.home.orders().is_empty());
}

#[test]
fn an_order_with_another_content_type_is_a_415_and_nothing_is_queued() {
    let (mut rig, id) = light();
    let path = format!("/api/v1/equipments/{id}/orders/state");
    for content_type in [
        Some("text/plain"),
        Some("application/x-www-form-urlencoded"),
        None,
    ] {
        let Reply::Whole(response) = post_order_typed(&mut rig, &path, content_type, true) else {
            panic!("an order was queued for {content_type:?}");
        };
        assert_eq!(response.status, 415);
        assert_eq!(
            response.body.as_deref(),
            Some(r#"{"error":"Unsupported Media Type"}"#)
        );
        assert!(rig.home.orders().is_empty());
    }
    // Parameters and case are allowed.
    let reply = post_order_typed(
        &mut rig,
        &path,
        Some("Application/JSON; charset=utf-8"),
        true,
    );
    assert!(matches!(reply, Reply::Order(_)));
}

#[test]
fn the_deadline_of_an_order_of_the_api_is_counted_on_the_queues_monotonic_clock() {
    let (mut rig, id) = light();
    let path = format!("/api/v1/equipments/{id}/orders/state");
    // The host's clock reads 5 s; the wall clock (`rig.now`) is far ahead of it.
    rig.home.orders().set_clock(std::sync::Arc::new(|| 5_000));
    let reply = post_order_typed(&mut rig, &path, Some("application/json"), true);
    assert!(matches!(reply, Reply::Order(_)));
    // Taken at the wall clock's time it would be long past its deadline (5 s + 12 s)...
    assert!(rig.home.orders().take(rig.now).is_none());
    let reply = post_order_typed(&mut rig, &path, Some("application/json"), true);
    assert!(matches!(reply, Reply::Order(_)));
    // ...on the monotonic clock it is taken.
    assert!(rig.home.orders().take(5_000 + DEADLINE_MS - 1).is_some());
}
