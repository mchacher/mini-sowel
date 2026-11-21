//! `--serve` on the simulated radio: the device API over HTTP, the devices kept between runs,
//! the usage errors and Ctrl-C. The HTTP client is a hand-written GET on a `TcpStream`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

const GARAGE: &str = "../../scenarios/garage.json";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn data_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mini-sowel-serve-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A port nobody listens on now.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn host() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
}

/// `--pair` on the garage scenario: three devices, written to `devices.json`.
fn pair(dir: &Path) {
    pair_scenario(dir, GARAGE);
}

fn pair_scenario(dir: &Path, scenario: &str) {
    let output = host()
        .args([
            "--sim", scenario, "--pair", "30", "--speed", "100", "--data",
        ])
        .arg(dir)
        .output()
        .expect("the host program starts");
    assert!(output.status.success(), "{output:?}");
}

/// A running `--serve`.
struct Serving {
    child: Child,
    port: u16,
    lines: Receiver<String>,
    shown: Vec<String>,
}

fn serve(dir: &Path, speed: &str, extra: &[&str]) -> Serving {
    serve_scenario(dir, GARAGE, speed, extra)
}

fn serve_scenario(dir: &Path, scenario: &str, speed: &str, extra: &[&str]) -> Serving {
    let port = free_port();
    let mut child = host()
        .args([
            "--sim",
            scenario,
            "--serve",
            &port.to_string(),
            "--speed",
            speed,
            "--data",
        ])
        .arg(dir)
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the host program starts");
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut serving = Serving {
        child,
        port,
        lines,
        shown: Vec::new(),
    };
    loop {
        match serving.lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => {
                let ready = line.starts_with("serving:");
                serving.shown.push(line);
                if ready {
                    return serving;
                }
            }
            Err(_) => {
                let _ = serving.child.kill();
                panic!("the host stopped or stayed silent: {:?}", serving.shown);
            }
        }
    }
}

impl Serving {
    /// Sends a request and returns (status, body); a chunked body is decoded.
    fn request(&self, method: &str, path: &str, host: &str, body: &str) -> (u16, String) {
        let (status, _, body) = self.request_full(method, path, host, body);
        (status, body)
    }

    /// Sends a request and returns (status, head, body; a chunked body decoded).
    fn request_full(
        &self,
        method: &str,
        path: &str,
        host: &str,
        body: &str,
    ) -> (u16, String, String) {
        self.request_typed(method, path, host, body, "application/json")
    }

    /// The same with the `Content-Type` of the request given.
    fn request_typed(
        &self,
        method: &str,
        path: &str,
        host: &str,
        body: &str,
        content_type: &str,
    ) -> (u16, String, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP answer");
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        let body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            dechunk(body)
        } else {
            body.to_string()
        };
        (status, head.to_string(), body)
    }

    fn get(&self, path: &str) -> (u16, String) {
        self.request("GET", path, &format!("127.0.0.1:{}", self.port), "")
    }

    fn devices(&self) -> Vec<Value> {
        let (status, body) = self.get("/api/v1/devices");
        assert_eq!(status, 200);
        serde_json::from_str::<Value>(&body)
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    }

    /// Ctrl-C, then the exit code.
    fn interrupt(mut self) -> Option<i32> {
        let signal = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(signal.success());
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code();
            }
            if start.elapsed() > Duration::from_secs(20) {
                let _ = self.child.kill();
                panic!("the host did not stop after Ctrl-C");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The body of a chunked answer: `<hex size>\r\n<data>\r\n` repeated, ending with a size 0.
fn dechunk(mut rest: &str) -> String {
    let mut out = String::new();
    loop {
        let (size, after) = rest.split_once("\r\n").expect("a chunk size");
        let size = usize::from_str_radix(size.trim(), 16).expect("a hex size");
        if size == 0 {
            return out;
        }
        out.push_str(&after[..size]);
        rest = &after[size + 2..];
    }
}

fn find<'a>(devices: &'a [Value], model: &str) -> &'a Value {
    devices.iter().find(|d| d["model"] == model).unwrap()
}

fn data<'a>(device: &'a Value, key: &str) -> &'a Value {
    device["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["key"] == key)
        .unwrap()
}

/// Waits until the temperature of the sensor is not `not`; returns it.
fn temperature_other_than(serving: &Serving, not: &Value) -> Value {
    let start = Instant::now();
    loop {
        let devices = serving.devices();
        let value = data(find(&devices, "TH01"), "temperature")["value"].clone();
        if &value != not && !value.is_null() {
            return value;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no new temperature"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn pairing_then_serving_lists_three_devices_and_the_temperature_changes() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "100", &[]);
    assert!(serving.shown[0].starts_with("mini-sowel "));
    assert_eq!(
        serving.shown.last().unwrap(),
        &format!("serving: http://127.0.0.1:{}", serving.port)
    );
    assert!(
        serving
            .shown
            .iter()
            .any(|l| l.starts_with("network: resumed"))
    );
    let devices = serving.devices();
    assert_eq!(devices.len(), 3);
    let relay = find(&devices, "ZBMINIR2");
    assert_eq!(relay["powerSource"], "mains");
    assert_eq!(data(relay, "state")["type"], "boolean");
    assert_eq!(relay["orders"][0]["key"], "state");
    assert_eq!(relay["orders"][0]["category"], "light_toggle");
    let motion = find(&devices, "MS01");
    assert_eq!(data(motion, "battery")["category"], "battery");
    assert_eq!(data(motion, "voltage")["unit"], "mV");
    assert_eq!(motion["integrationId"], "mini-sowel");
    assert!(motion["zoneId"].is_null());

    let first = data(find(&devices, "TH01"), "temperature")["value"].clone();
    let next = temperature_other_than(&serving, &first);
    assert_ne!(next, first);

    // One device by id; an unknown id.
    let id = relay["id"].as_str().unwrap();
    let (status, body) = serving.get(&format!("/api/v1/devices/{id}"));
    assert_eq!(status, 200);
    assert!(body.contains("\"orders\""));
    assert_eq!(serving.get("/api/v1/devices/nope").0, 404);
    // A page from another origin cannot reach it.
    let (status, body) = serving.request("GET", "/api/v1/devices", "evil.example", "");
    assert_eq!(
        (status, body.as_str()),
        (403, r#"{"error":"Forbidden host"}"#)
    );
    // A rename reaches the file at once.
    let (status, _) = serving.request(
        "PUT",
        &format!("/api/v1/devices/{id}"),
        &format!("127.0.0.1:{}", serving.port),
        r#"{"name":"Garage light"}"#,
    );
    assert_eq!(status, 200);
    let start = Instant::now();
    while !std::fs::read_to_string(dir.join("devices.json"))
        .unwrap()
        .contains("Garage light")
    {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "rename not written"
        );
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn a_port_in_use_is_the_error_and_exit_1() {
    let dir = data_dir();
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let output = host()
        .args(["--sim", GARAGE, "--serve", &port.to_string(), "--data"])
        .arg(&dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with(&format!("serve: cannot listen on 127.0.0.1:{port}: ")),
        "{stderr}"
    );
}

#[test]
fn serve_with_pair_with_listen_or_without_a_radio_is_a_usage_error() {
    for args in [
        vec!["--sim", GARAGE, "--serve", "--pair"],
        vec!["--sim", GARAGE, "--serve", "--listen"],
        vec!["--serve"],
        vec!["--sim", GARAGE, "--serve", "0"],
        vec!["--sim", GARAGE, "--verbose"],
    ] {
        let output = host().args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .starts_with("usage:")
        );
    }
}

#[test]
fn restarting_serve_keeps_the_devices_their_ids_and_their_last_values() {
    let dir = data_dir();
    pair(&dir);
    let first = serve(&dir, "100", &[]);
    let before = first.devices();
    let value = temperature_other_than(&first, &Value::Null);
    assert_eq!(first.interrupt(), Some(130));
    // Speed 1: no new report within the test, so what is served is what was kept.
    let second = serve(&dir, "1", &[]);
    let after = second.devices();
    let ids = |devices: &[Value]| -> Vec<Value> {
        let mut ids: Vec<Value> = devices.iter().map(|d| d["id"].clone()).collect();
        ids.sort_by_key(ToString::to_string);
        ids
    };
    assert_eq!(ids(&before), ids(&after));
    let kept = data(find(&after, "TH01"), "temperature")["value"].clone();
    assert!(!kept.is_null(), "a value was kept (was {value})");
    assert_eq!(second.interrupt(), Some(130));
}

#[test]
fn ctrl_c_while_serving_writes_the_store_and_exits_130() {
    let dir = data_dir();
    pair(&dir);
    let before = std::fs::read_to_string(dir.join("devices.json")).unwrap();
    let serving = serve(&dir, "100", &["--verbose"]);
    // The measures are shown too with --verbose.
    let start = Instant::now();
    let mut seen = Vec::new();
    while !seen.iter().any(|l: &String| l.contains("temperature")) {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "no measure line: {seen:?}"
        );
        if let Ok(line) = serving.lines.recv_timeout(Duration::from_millis(200)) {
            seen.push(line);
        }
    }
    assert_eq!(serving.interrupt(), Some(130));
    let after = std::fs::read_to_string(dir.join("devices.json")).unwrap();
    assert_ne!(
        before, after,
        "the values seen while serving were written on stop"
    );
}

#[test]
fn without_verbose_the_measure_lines_are_not_printed() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "100", &[]);
    thread::sleep(Duration::from_secs(2));
    let mut seen = serving.shown.clone();
    while let Ok(line) = serving.lines.try_recv() {
        seen.push(line);
    }
    assert!(!seen.iter().any(|l| l.contains("temperature")), "{seen:?}");
    serving.interrupt();
}

#[test]
fn a_devices_file_that_does_not_parse_stops_serve_and_pair_and_is_left_alone() {
    for mode in [vec!["--serve", "18999"], vec!["--pair", "5"]] {
        let dir = data_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("devices.json");
        std::fs::write(&file, "[ broken").unwrap();
        let output = host()
            .args(["--sim", GARAGE])
            .args(&mode)
            .arg("--data")
            .arg(&dir)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{mode:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.starts_with(&format!("devices: {}: cannot read: ", file.display())),
            "{stderr}"
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "[ broken");
        assert!(
            !dir.join("network.json").exists(),
            "the radio was not touched"
        );
    }
}

#[test]
fn a_client_that_stalls_in_its_body_blocks_neither_other_requests_nor_ctrl_c() {
    let dir = data_dir();
    pair(&dir);
    let before = std::fs::read_to_string(dir.join("devices.json")).unwrap();
    let serving = serve(&dir, "100", &[]);
    // Promises 10000 bytes, sends one, and stays connected.
    let mut stalled = TcpStream::connect(("127.0.0.1", serving.port)).unwrap();
    write!(
        stalled,
        "PUT /api/v1/devices/x HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: 10000\r\n\r\n{{",
        serving.port
    )
    .unwrap();
    thread::sleep(Duration::from_millis(300));
    // Another client is still answered, and the values keep coming.
    assert_eq!(serving.devices().len(), 3);
    let first = data(find(&serving.devices(), "TH01"), "temperature")["value"].clone();
    temperature_other_than(&serving, &first);
    // Ctrl-C: saved and out with 130, the stalled client still connected.
    assert_eq!(serving.interrupt(), Some(130));
    let after = std::fs::read_to_string(dir.join("devices.json")).unwrap();
    assert_ne!(before, after, "the store was written on stop");
    drop(stalled);
}

/// Waits until the `occupancy` of the motion sensor is `wanted`.
fn wait_for_occupancy(serving: &Serving, wanted: bool) {
    let start = Instant::now();
    loop {
        let devices = serving.devices();
        if data(find(&devices, "MS01"), "occupancy")["value"] == wanted {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "occupancy never became {wanted}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_motion_sensor_enrolled_at_pairing_sets_occupancy_true_on_motion_and_false_a_minute_later() {
    let dir = data_dir();
    pair(&dir);
    // Speed 20: motion at 1 min of the scenario comes after 3 s, the clearing 60 s of scenario
    // time (3 s) after it.
    let serving = serve(&dir, "20", &[]);
    let devices = serving.devices();
    let motion = find(&devices, "MS01");
    assert_eq!(data(motion, "occupancy")["category"], "motion");
    assert_eq!(data(motion, "occupancy")["type"], "boolean");
    wait_for_occupancy(&serving, true);
    // The device API shows the sensor as the spec says, after its motion event.
    let devices = serving.devices();
    assert_eq!(data(find(&devices, "MS01"), "occupancy")["value"], true);
    wait_for_occupancy(&serving, false);
    assert_eq!(serving.interrupt(), Some(130));
}

// ---- Spec 010: zones, equipments, aggregation ----

impl Serving {
    fn send(&self, method: &str, path: &str, body: &Value) -> (u16, Value) {
        let (status, text) = self.request(
            method,
            path,
            &format!("127.0.0.1:{}", self.port),
            &body.to_string(),
        );
        let json = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap()
        };
        (status, json)
    }

    /// `GET /api/v1/zones/aggregation`.
    fn aggregation(&self) -> Value {
        let (status, body) = self.get("/api/v1/zones/aggregation");
        assert_eq!(status, 200);
        serde_json::from_str(&body).unwrap()
    }
}

/// A zone "Garage", the three devices in it and the three equipments of the demo; the zone id.
fn set_up_the_garage(serving: &Serving) -> String {
    let (status, zone) = serving.send(
        "POST",
        "/api/v1/zones",
        &serde_json::json!({"name": "Garage"}),
    );
    assert_eq!(status, 201, "{zone}");
    let zone = zone["id"].as_str().unwrap().to_string();
    let devices = serving.devices();
    for model in ["TH01", "MS01", "ZBMINIR2"] {
        let id = find(&devices, model)["id"].as_str().unwrap();
        let (status, body) = serving.send(
            "PUT",
            &format!("/api/v1/devices/{id}"),
            &serde_json::json!({ "zoneId": zone }),
        );
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["zoneId"], zone.as_str());
    }
    for (name, kind, model) in [
        ("Garage climate", "sensor", "TH01"),
        ("Garage motion", "sensor", "MS01"),
        ("Garage light", "light_onoff", "ZBMINIR2"),
    ] {
        let id = find(&devices, model)["id"].as_str().unwrap();
        let (status, body) = serving.send(
            "POST",
            "/api/v1/equipments",
            &serde_json::json!({"name": name, "type": kind, "zoneId": zone, "deviceIds": [id]}),
        );
        assert_eq!(status, 201, "{body}");
        assert!(!body["dataBindings"].as_array().unwrap().is_empty());
    }
    zone
}

/// Waits until the zone has a temperature and motion; its aggregated data.
fn wait_for_motion(serving: &Serving, zone: &str) -> Value {
    let start = Instant::now();
    loop {
        let aggregation = serving.aggregation();
        let data = &aggregation[zone];
        if data["motion"] == true && !data["temperature"].is_null() {
            return aggregation;
        }
        assert!(
            start.elapsed() < Duration::from_secs(40),
            "no motion with a temperature: {data}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_garage_zone_three_equipments_and_the_aggregation_of_the_garage_and_the_root() {
    let dir = data_dir();
    pair(&dir);
    assert!(
        !dir.join("zones.json").exists(),
        "--pair writes the devices only"
    );
    let serving = serve(&dir, "10", &[]);
    // The root zone exists from the first --serve, and is on disk at once.
    let (status, tree) = serving.send("GET", "/api/v1/zones", &Value::Null);
    assert_eq!(status, 200);
    assert_eq!(tree[0]["name"], "Maison");
    assert_eq!(tree[0]["id"], "00000000-0000-0000-0000-000000000001");
    let start = Instant::now();
    while !dir.join("zones.json").exists() {
        assert!(start.elapsed() < Duration::from_secs(10), "no zones.json");
        thread::sleep(Duration::from_millis(50));
    }

    let zone = set_up_the_garage(&serving);
    let aggregation = wait_for_motion(&serving, &zone);
    for id in [zone.as_str(), "00000000-0000-0000-0000-000000000001"] {
        let data = &aggregation[id];
        let temperature = data["temperature"].as_f64().unwrap();
        assert!((12.0..=14.7).contains(&temperature), "{id}: {temperature}");
        // One decimal.
        assert_eq!((temperature * 10.0).round() / 10.0, temperature);
        assert_eq!(data["motion"], true);
        assert_eq!(data["motionSensors"], 1);
        assert!(data["motionSince"].is_string(), "{id}");
        assert_eq!(data["lightsTotal"], 1);
        assert_eq!(data["lightsOn"], 0);
        assert!(data["humidity"].as_f64().is_some());
    }
    // The equipments, with the status of a device that talked recently.
    let (_, equipments) = serving.send("GET", "/api/v1/equipments", &Value::Null);
    let names: Vec<&str> = equipments
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Garage climate", "Garage light", "Garage motion"]);
    let climate = &equipments[0];
    assert_eq!(climate["status"], "online");
    let temperature = climate["dataBindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["alias"] == "temperature")
        .unwrap();
    assert!(temperature["value"].is_number());
    assert!(temperature["lastChanged"].is_string());
    assert_eq!(temperature["stale"], false);
    assert_eq!(serving.interrupt(), Some(130));

    // The files are on disk, readable by the owner only.
    let zones = std::fs::read_to_string(dir.join("zones.json")).unwrap();
    assert!(zones.contains("Garage") && zones.contains("Maison"));
    let equipments = std::fs::read_to_string(dir.join("equipments.json")).unwrap();
    assert!(equipments.contains("Garage light") && equipments.contains("light_onoff"));
    use std::os::unix::fs::PermissionsExt;
    for name in ["zones.json", "equipments.json", "devices.json"] {
        let mode = std::fs::metadata(dir.join(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "{name}");
    }
}

#[test]
fn restarting_serve_keeps_the_zones_the_equipments_and_the_devices_in_their_zones() {
    let dir = data_dir();
    pair(&dir);
    let first = serve(&dir, "100", &[]);
    let zone = set_up_the_garage(&first);
    let (_, before) = first.send("GET", "/api/v1/equipments", &Value::Null);
    assert_eq!(first.interrupt(), Some(130));

    // Speed 1: nothing new within the test, so what is served is what was kept.
    let second = serve(&dir, "1", &[]);
    let (_, tree) = second.send("GET", "/api/v1/zones", &Value::Null);
    assert_eq!(tree[0]["children"][0]["id"], zone.as_str());
    assert_eq!(tree[0]["children"][0]["name"], "Garage");
    let (_, after) = second.send("GET", "/api/v1/equipments", &Value::Null);
    let strip = |list: &Value| -> Vec<Value> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|e| {
                // The ids of the equipments and the bindings, and their zone.
                serde_json::json!({
                    "id": e["id"], "name": e["name"], "zoneId": e["zoneId"], "type": e["type"],
                    "bindings": e["dataBindings"].as_array().unwrap().iter()
                        .map(|b| b["id"].clone()).collect::<Vec<_>>(),
                })
            })
            .collect()
    };
    assert_eq!(strip(&before), strip(&after));
    for device in second.devices() {
        assert_eq!(device["zoneId"], zone.as_str());
    }
    assert!(second.aggregation()[zone.as_str()]["lightsTotal"] == 1);
    assert_eq!(second.interrupt(), Some(130));
}

#[test]
fn a_zones_or_equipments_file_that_does_not_parse_stops_serve_and_pair_and_is_left_alone() {
    for name in ["zones.json", "equipments.json"] {
        for mode in [vec!["--serve", "18998"], vec!["--pair", "5"]] {
            let dir = data_dir();
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join(name);
            std::fs::write(&file, "[ broken").unwrap();
            let output = host()
                .args(["--sim", GARAGE])
                .args(&mode)
                .arg("--data")
                .arg(&dir)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{name} {mode:?}");
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(
                stderr.starts_with(&format!(
                    "{}: {}: cannot read: ",
                    name.trim_end_matches(".json"),
                    file.display()
                )),
                "{stderr}"
            );
            assert_eq!(std::fs::read_to_string(&file).unwrap(), "[ broken");
            assert!(
                !dir.join("network.json").exists(),
                "the radio was not touched"
            );
        }
    }
}

#[test]
fn the_new_routes_refuse_a_foreign_host_and_answer_json_errors_over_http() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "100", &[]);
    for path in [
        "/api/v1/zones",
        "/api/v1/equipments",
        "/api/v1/zones/aggregation",
    ] {
        let (status, body) = serving.request("GET", path, "evil.example", "");
        assert_eq!(
            (status, body.as_str()),
            (403, r#"{"error":"Forbidden host"}"#),
            "{path}"
        );
    }
    let (status, body) = serving.send("POST", "/api/v1/zones", &serde_json::json!({"color": 1}));
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("Unknown field: color"))
    );
    let (status, body) = serving.send("GET", "/api/v1/equipments/nope", &Value::Null);
    assert_eq!(
        (status, body["error"].as_str()),
        (404, Some("Equipment not found"))
    );
    // Concurrent creations from several clients: every id is distinct (the shared random source).
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let port = serving.port;
            thread::spawn(move || {
                let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
                let body = format!(r#"{{"name":"Zone {i}"}}"#);
                write!(
                    stream,
                    "POST /api/v1/zones HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                let mut text = String::new();
                stream.read_to_string(&mut text).unwrap();
                text.contains(" 201 ")
            })
        })
        .collect();
    let created = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert!(created >= 1, "some creations were answered");
    let (_, tree) = serving.send("GET", "/api/v1/zones", &Value::Null);
    let mut ids: Vec<String> = tree[0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|z| z["id"].as_str().unwrap().to_string())
        .collect();
    let count = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), count, "no id twice");
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn motion_since_is_the_time_the_radio_loop_saw_the_change_not_the_time_somebody_asked() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "10", &[]);
    let zone = set_up_the_garage(&serving);
    // Nobody asks for the aggregation: the device API is read until the motion is there.
    wait_for_occupancy(&serving, true);
    thread::sleep(Duration::from_millis(700));
    let (_, equipments) = serving.send("GET", "/api/v1/equipments", &Value::Null);
    let aggregation = serving.aggregation();
    let motion = equipments
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Garage motion")
        .unwrap();
    let binding = &motion["dataBindings"][0];
    assert_eq!(binding["category"], "motion");
    assert_eq!(
        aggregation[zone.as_str()]["motion"],
        true,
        "still within its minute"
    );
    // The change was seen by the radio loop at one instant: the binding's `lastChanged` and the
    // zone's `motionSince` are that instant, however long after it is read.
    assert_eq!(
        aggregation[zone.as_str()]["motionSince"],
        binding["lastChanged"]
    );
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn the_long_lists_come_chunked_and_are_the_same_json_with_filter_sorting_and_empty_lists() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "1", &[]);
    let zone = set_up_the_garage(&serving);
    let host = format!("127.0.0.1:{}", serving.port);
    // Chunked, without a length, as JSON.
    for path in ["/api/v1/devices", "/api/v1/equipments"] {
        let (status, head, body) = serving.request_full("GET", path, &host, "");
        assert_eq!(status, 200);
        let head = head.to_ascii_lowercase();
        assert!(head.contains("transfer-encoding: chunked"), "{head}");
        assert!(!head.contains("content-length"), "{head}");
        assert!(
            head.contains("content-type: application/json; charset=utf-8"),
            "{head}"
        );
        assert!(serde_json::from_str::<Value>(&body).unwrap().is_array());
    }
    // The devices, as they were; the equipments by name.
    assert_eq!(serving.devices().len(), 3);
    let (_, equipments) = serving.send("GET", "/api/v1/equipments", &Value::Null);
    let names: Vec<&str> = equipments
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Garage climate", "Garage light", "Garage motion"]);
    // The filter, and a type that does not exist.
    let (_, sensors) = serving.send("GET", "/api/v1/equipments?type=sensor", &Value::Null);
    assert_eq!(sensors.as_array().unwrap().len(), 2);
    let (status, none) = serving.send("GET", "/api/v1/equipments?type=nothing", &Value::Null);
    assert_eq!((status, none), (200, serde_json::json!([])));
    let (status, text) = serving.request("GET", "/api/v1/equipments?type=nothing", &host, "");
    assert_eq!((status, text.as_str()), (200, "[]"));
    // More than one chunk: 3 + 12 equipments, still sorted and whole.
    for i in 0..12 {
        let (status, _) = serving.send(
            "POST",
            "/api/v1/equipments",
            &serde_json::json!({"name": format!("Extra {i:02}"), "type": "switch", "zoneId": zone}),
        );
        assert_eq!(status, 201);
    }
    let (_, all) = serving.send("GET", "/api/v1/equipments", &Value::Null);
    let all = all.as_array().unwrap();
    assert_eq!(all.len(), 15);
    let names: Vec<&str> = all.iter().map(|e| e["name"].as_str().unwrap()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert!(
        all.iter()
            .all(|e| e["status"].is_string() && e["dataBindings"].is_array())
    );
    assert_eq!(serving.interrupt(), Some(130));
}

// ---- Spec 011: orders ----

const ROOT: &str = "00000000-0000-0000-0000-000000000001";
/// The garage relay's address as the simulated radio keeps it (little-endian).
const RELAY_WIRE: &str = "7A563412004B1200";

/// A light equipment (in the root zone) on the device of this model; its id.
fn light_on(serving: &Serving, model: &str, kind: &str) -> String {
    let devices = serving.devices();
    let id = find(&devices, model)["id"].as_str().unwrap();
    let (status, body) = serving.send(
        "POST",
        "/api/v1/equipments",
        &serde_json::json!({"name": "Light", "type": kind, "zoneId": ROOT, "deviceIds": [id]}),
    );
    assert_eq!(status, 201, "{body}");
    body["id"].as_str().unwrap().to_string()
}

fn order(serving: &Serving, equipment: &str, alias: &str, body: &str) -> (u16, Value) {
    post_order(serving.port, equipment, alias, body)
}

/// `POST .../orders/<alias>` on a port (a thread has no `Serving`).
fn post_order(port: u16, equipment: &str, alias: &str, body: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    write!(
        stream,
        "POST /api/v1/equipments/{equipment}/orders/{alias} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut text = String::new();
    stream.read_to_string(&mut text).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP answer");
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// The value of a binding of an equipment.
fn binding_value(serving: &Serving, equipment: &str, alias: &str) -> Value {
    let (status, body) = serving.send(
        "GET",
        &format!("/api/v1/equipments/{equipment}"),
        &Value::Null,
    );
    assert_eq!(status, 200);
    body["dataBindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["alias"] == alias)
        .map_or(Value::Null, |b| b["value"].clone())
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(20), "{what}");
        thread::sleep(Duration::from_millis(50));
    }
}

/// Puts a switch in the simulated radio's memory (`sim-garage.json`): the relay answers nothing.
fn silence_the_relay(dir: &Path) {
    switch_the_relay(dir, "silent");
}

/// Sets a boolean switch of the relay in the simulated radio's memory.
fn switch_the_relay(dir: &Path, switch: &str) {
    let file = dir.join("sim-garage.json");
    let mut memory: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let relay = memory["devices"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["eui64Wire"] == RELAY_WIRE)
        .expect("the relay joined");
    relay[switch] = Value::Bool(true);
    std::fs::write(&file, memory.to_string()).unwrap();
}

#[test]
fn an_order_on_the_sim_relay_switches_it_the_state_and_the_lights_on_follow() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "100", &[]);
    let zone = set_up_the_garage(&serving);
    let (_, equipments) = serving.send("GET", "/api/v1/equipments", &Value::Null);
    let light = equipments
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Garage light")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let lights_on = |serving: &Serving| serving.aggregation()[zone.as_str()]["lightsOn"].clone();
    assert_eq!(lights_on(&serving), 0);

    let (status, body) = order(&serving, &light, "state", r#"{"value":true}"#);
    assert_eq!((status, body), (200, serde_json::json!({"success": true})));
    wait_until("state true", || {
        binding_value(&serving, &light, "state") == true
    });
    wait_until("lightsOn 1", || lights_on(&serving) == 1);
    assert_eq!(serving.aggregation()[ROOT]["lightsOn"], 1);
    let (status, body) = order(&serving, &light, "state", r#"{"value":false}"#);
    assert_eq!((status, body), (200, serde_json::json!({"success": true})));
    wait_until("state false", || {
        binding_value(&serving, &light, "state") == false
    });
    wait_until("lightsOn 0", || lights_on(&serving) == 0);

    // Sowel's refusals, over HTTP.
    let (status, body) = order(&serving, &light, "state", r#"{"value":"x"}"#);
    assert_eq!(
        (status, body),
        (400, serde_json::json!({"error": "Invalid value for state"}))
    );
    let (status, body) = order(&serving, &light, "nothing", "{}");
    assert_eq!(
        (status, body),
        (
            404,
            serde_json::json!({"error": "Order alias not found: nothing"})
        )
    );
    let (status, body) = order(
        &serving,
        "11111111-2222-4333-8444-555555555555",
        "state",
        "{}",
    );
    assert_eq!(
        (status, body),
        (404, serde_json::json!({"error": "Equipment not found"}))
    );
    let (status, _) = serving.send(
        "PUT",
        &format!("/api/v1/equipments/{light}"),
        &serde_json::json!({"enabled": false}),
    );
    assert_eq!(status, 200);
    let (status, body) = order(&serving, &light, "state", "{}");
    assert_eq!(
        (status, body),
        (400, serde_json::json!({"error": "Equipment is disabled"}))
    );
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn a_relay_not_heard_since_the_start_is_reached_at_the_address_devices_json_keeps() {
    let dir = data_dir();
    pair(&dir);
    // The address and endpoint the pairing learned are in devices.json, beside Sowel's fields.
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("devices.json")).unwrap()).unwrap();
    let relay = stored
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["model"] == "ZBMINIR2")
        .unwrap();
    assert!(relay["nwk"].is_u64(), "{relay}");
    assert_eq!(relay["endpoint"], 1);
    // The radio's address table does not know the relay: only devices.json does.
    switch_the_relay(&dir, "lookupUnknown");
    // Speed 1: the scenario's reports are far away, the relay says nothing in this run before
    // the order.
    let serving = serve(&dir, "1", &[]);
    let light = light_on(&serving, "ZBMINIR2", "light_onoff");
    let (status, body) = order(&serving, &light, "state", r#"{"value":true}"#);
    assert_eq!((status, body), (200, serde_json::json!({"success": true})));
    wait_until("state true", || {
        binding_value(&serving, &light, "state") == true
    });
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn a_silenced_relay_is_a_502_device_did_not_answer_after_the_retry() {
    let dir = data_dir();
    pair(&dir);
    silence_the_relay(&dir);
    let serving = serve(&dir, "100", &[]);
    let light = light_on(&serving, "ZBMINIR2", "light_onoff");
    let started = Instant::now();
    let (status, body) = order(&serving, &light, "state", r#"{"value":true}"#);
    let took = started.elapsed();
    assert_eq!(
        (status, body),
        (502, serde_json::json!({"error": "device did not answer"}))
    );
    // 5 s, the retry 2 s later, 5 s more: the answer comes at the deadline, not much later.
    assert!(
        took >= Duration::from_secs(11) && took < Duration::from_secs(14),
        "{took:?}"
    );
    // The relay never switched: its state is not true.
    assert_ne!(binding_value(&serving, &light, "state"), true);
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn a_post_that_is_not_json_is_a_415_and_an_order_of_that_kind_sends_nothing() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "100", &[]);
    let light = light_on(&serving, "ZBMINIR2", "light_onoff");
    let host = format!("127.0.0.1:{}", serving.port);
    let before = binding_value(&serving, &light, "state");
    let started = Instant::now();
    for path in [
        format!("/api/v1/equipments/{light}/orders/state"),
        "/api/v1/zones".to_string(),
    ] {
        let (status, _, body) =
            serving.request_typed("POST", &path, &host, r#"{"value":true}"#, "text/plain");
        assert_eq!(
            (status, body.as_str()),
            (415, r#"{"error":"Unsupported Media Type"}"#)
        );
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "answered at once"
    );
    // Nothing was sent: the relay's state is what it was.
    assert_eq!(binding_value(&serving, &light, "state"), before);
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn a_third_order_is_busy_and_reads_are_still_served_while_two_wait_on_a_silenced_relay() {
    let dir = data_dir();
    pair(&dir);
    silence_the_relay(&dir);
    let serving = serve(&dir, "100", &[]);
    let light = light_on(&serving, "ZBMINIR2", "light_onoff");
    let mut waiting = Vec::new();
    for _ in 0..2 {
        let (port, light) = (serving.port, light.clone());
        waiting.push(thread::spawn(move || {
            post_order(port, &light, "state", r#"{"value":true}"#)
        }));
        thread::sleep(Duration::from_millis(200));
    }
    let started = Instant::now();
    let (status, body) = order(&serving, &light, "state", r#"{"value":true}"#);
    assert_eq!((status, body), (503, serde_json::json!({"error": "Busy"})));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "refused at once"
    );
    // The reads are served meanwhile.
    assert_eq!(serving.devices().len(), 3);
    for handle in waiting {
        let (status, body) = handle.join().unwrap();
        assert_eq!(
            (status, body),
            (502, serde_json::json!({"error": "device did not answer"}))
        );
    }
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn a_relay_whose_address_is_unknown_to_the_registry_and_the_radio_is_a_502_device_address_unknown()
{
    let dir = data_dir();
    pair(&dir);
    // The registry loses the relay's address and the radio does not know the device any more:
    // its address in devices.json is one the radio never saw.
    let file = dir.join("devices.json");
    let mut stored: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let relay = stored
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["model"] == "ZBMINIR2")
        .unwrap();
    relay["sourceDeviceId"] = Value::String("0x00124b00deadbeef".into());
    relay.as_object_mut().unwrap().remove("nwk");
    relay.as_object_mut().unwrap().remove("endpoint");
    std::fs::write(&file, stored.to_string()).unwrap();
    let serving = serve(&dir, "100", &[]);
    let light = light_on(&serving, "ZBMINIR2", "light_onoff");
    let started = Instant::now();
    let (status, body) = order(&serving, &light, "state", r#"{"value":true}"#);
    assert_eq!(
        (status, body),
        (502, serde_json::json!({"error": "device address unknown"}))
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(serving.interrupt(), Some(130));
}

const DIMMER: &str = r#"{
  "devices": [ { "id": "lamp", "kind": "dimmer", "eui64": "00124B00123456B1" } ],
  "events": [ { "at": "2s", "device": "lamp", "do": "join" } ]
}"#;

#[test]
fn brightness_on_the_sim_dimmer_is_a_200_and_the_brightness_is_reported() {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let scenario = dir.join("dimmer.json");
    std::fs::write(&scenario, DIMMER).unwrap();
    let scenario = scenario.to_str().unwrap();
    pair_scenario(&dir, scenario);
    let serving = serve_scenario(&dir, scenario, "100", &[]);
    let lamp = light_on(&serving, "DIM01", "light_dimmable");
    let (status, body) = order(&serving, &lamp, "brightness", r#"{"value":200}"#);
    assert_eq!((status, body), (200, serde_json::json!({"success": true})));
    wait_until("brightness 200", || {
        binding_value(&serving, &lamp, "brightness") == 200
    });
    assert_eq!(binding_value(&serving, &lamp, "state"), true);
    // 255 counts as 254; 0 switches it off.
    let (status, _) = order(&serving, &lamp, "brightness", r#"{"value":255}"#);
    assert_eq!(status, 200);
    wait_until("brightness 254", || {
        binding_value(&serving, &lamp, "brightness") == 254
    });
    let (status, _) = order(&serving, &lamp, "brightness", r#"{"value":0}"#);
    assert_eq!(status, 200);
    wait_until("state false", || {
        binding_value(&serving, &lamp, "state") == false
    });
    let (status, body) = order(&serving, &lamp, "brightness", r#"{"value":300}"#);
    assert_eq!(
        (status, body),
        (
            400,
            serde_json::json!({"error": "Invalid value for brightness"})
        )
    );
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn an_order_is_answered_while_slow_clients_hold_the_long_lists_open() {
    let dir = data_dir();
    pair(&dir);
    let serving = serve(&dir, "100", &[]);
    let light = light_on(&serving, "ZBMINIR2", "light_onoff");
    // As many equipments as the home holds, so the list is long.
    let devices = serving.devices();
    let ids: Vec<String> = devices
        .iter()
        .map(|d| d["id"].as_str().unwrap().to_string())
        .collect();
    for i in 0..60 {
        let (status, _) = serving.send(
            "POST",
            "/api/v1/equipments",
            &serde_json::json!({"name": format!("E{i:02}"), "type": "sensor", "zoneId": ROOT, "deviceIds": ids}),
        );
        assert_eq!(status, 201);
    }
    // Two clients ask for the list and read none of it.
    let mut slow = Vec::new();
    for _ in 0..2 {
        let mut stream = TcpStream::connect(("127.0.0.1", serving.port)).unwrap();
        write!(
            stream,
            "GET /api/v1/equipments HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            serving.port
        )
        .unwrap();
        slow.push(stream);
    }
    thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    let (status, body) = order(&serving, &light, "state", r#"{"value":true}"#);
    assert_eq!((status, body), (200, serde_json::json!({"success": true})));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    drop(slow);
    assert_eq!(serving.interrupt(), Some(130));
}
