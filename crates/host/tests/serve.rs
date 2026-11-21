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
    let output = host()
        .args(["--sim", GARAGE, "--pair", "30", "--speed", "100", "--data"])
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
    let port = free_port();
    let mut child = host()
        .args([
            "--sim",
            GARAGE,
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
    /// Sends a request and returns (status, body).
    fn request(&self, method: &str, path: &str, host: &str, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP answer");
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, body.to_string())
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
        "PUT /api/v1/devices/x HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 10000\r\n\r\n{{",
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
