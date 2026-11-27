//! `scripts/measure-api.sh` (spec 022 R10.2-3): the 17 requests of an equipment with 8 + 8
//! bindings, against the Mac host and against small test servers for the 503 and slow cases.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

const GARAGE: &str = "../../scenarios/garage.json";
const ONE_SENSOR: &str = "../../scenarios/snzb02.json";
const SCRIPT: &str = "../../scripts/measure-api.sh";
/// The synthetic password of the tests' account.
const PASSWORD: &str = "test-password-1234";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn host() -> Command {
    Command::new(env!("CARGO_BIN_EXE_solite-host"))
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Pairs the scenario's devices, then serves; returns once the host says `serving:`.
fn serving(scenario: &str) -> (Reaped, u16, TempDir) {
    let dir = std::env::temp_dir().join(format!(
        "solite-measure-script-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let data = TempDir(dir);
    let output = host()
        .env("SOLITE_CLOSE_GRACE_SECONDS", "1")
        .args(["--sim", scenario, "--pair", "2", "--speed", "100", "--data"])
        .arg(&data.0)
        .output()
        .expect("the host program starts");
    assert!(output.status.success(), "{output:?}");
    let port = free_port();
    let mut child = Reaped(
        host()
            .args([
                "--sim",
                scenario,
                "--serve",
                &port.to_string(),
                "--speed",
                "100",
                "--data",
            ])
            .arg(&data.0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the host program starts"),
    );
    let stdout = BufReader::new(child.0.stdout.take().unwrap());
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    while let Ok(line) = lines.recv_timeout(Duration::from_secs(20)) {
        if line.starts_with("serving:") {
            return (child, port, data);
        }
    }
    panic!("the host stopped or stayed silent");
}

/// (success, standard output, standard error).
fn run(args: &[&str]) -> (bool, String, String) {
    run_with(args, Some(PASSWORD))
}

/// The same with `SOLITE_PASSWORD` set to this, or unset.
fn run_with(args: &[&str], password: Option<&str>) -> (bool, String, String) {
    let (status, out, err) = run_status(args, password);
    (status == Some(0), out, err)
}

fn run_status(args: &[&str], password: Option<&str>) -> (Option<i32>, String, String) {
    let mut command = Command::new("sh");
    command.arg(SCRIPT).args(args).env_remove("SOLITE_PASSWORD");
    if let Some(password) = password {
        command.env("SOLITE_PASSWORD", password);
    }
    let output = command.output().expect("sh runs the script");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A GET with the access token of the account the script set up (a login of the test's own).
fn get(port: u16, path: &str) -> String {
    let login = Command::new("curl")
        .args(["-s", "-H", "Content-Type: application/json", "-d"])
        .arg(format!(r#"{{"username":"admin","password":"{PASSWORD}"}}"#))
        .arg(format!("http://127.0.0.1:{port}/api/v1/auth/login"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&login.stdout).into_owned();
    let tokens: serde_json::Value = serde_json::from_str(&text).expect(&text);
    let output = Command::new("curl")
        .args([
            "-s",
            "-H",
            &format!(
                "Authorization: Bearer {}",
                tokens["accessToken"].as_str().unwrap()
            ),
        ])
        .arg(format!("http://127.0.0.1:{port}/api/v1/{path}"))
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn measure_zone_count(port: u16) -> usize {
    get(port, "zones").matches("\"Measure 33\"").count()
}

#[test]
fn against_a_host_the_17_requests_all_answer_201_and_nothing_is_left_behind() {
    let (_host, port, _data) = serving(GARAGE);
    let (ok, out, err) = run(&[&port.to_string()]);
    assert!(ok, "{out}\n{err}");
    assert!(
        out.contains("measure: 17 requests, 17 answered 201, 0 Busy, total "),
        "{out}"
    );
    assert!(
        out.contains("slowest ") && out.contains("(request "),
        "{out}"
    );
    assert_eq!(measure_zone_count(port), 0);
    assert!(!get(port, "equipments").contains("Measure 33"));
}

#[test]
fn keep_leaves_the_zone_and_the_equipment_with_8_and_8_bindings() {
    let (_host, port, _data) = serving(GARAGE);
    let (ok, out, err) = run(&[&format!("127.0.0.1:{port}"), "--keep"]);
    assert!(ok, "{out}\n{err}");
    assert_eq!(measure_zone_count(port), 1);
    let equipments: serde_json::Value = serde_json::from_str(&get(port, "equipments")).unwrap();
    let kept = equipments
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Measure 33")
        .expect("the equipment is kept");
    assert_eq!(kept["dataBindings"].as_array().unwrap().len(), 8);
    assert_eq!(kept["orderBindings"].as_array().unwrap().len(), 8);
}

#[test]
fn an_existing_zone_measure_33_is_refused_and_nothing_is_created() {
    let (_host, port, _data) = serving(GARAGE);
    let (ok, _, _) = run(&[&port.to_string(), "--keep"]);
    assert!(ok);
    // The ids only: a simulated sensor keeps changing its values while the test runs.
    let ids = |port: u16| -> Vec<String> {
        let all: serde_json::Value = serde_json::from_str(&get(port, "equipments")).unwrap();
        all.as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap().to_string())
            .collect()
    };
    let equipments_before = ids(port);
    let (ok, out, err) = run(&[&port.to_string()]);
    assert!(!ok, "{out}");
    assert!(
        err.contains("a zone named Measure 33 exists already"),
        "{err}"
    );
    assert_eq!(measure_zone_count(port), 1, "no second zone");
    assert_eq!(
        ids(port),
        equipments_before,
        "no equipment created or removed"
    );
}

#[test]
fn with_no_device_with_an_order_it_refuses_before_creating_anything() {
    let (_host, port, _data) = serving(ONE_SENSOR);
    let (ok, _, err) = run(&[&port.to_string()]);
    assert!(!ok);
    assert!(
        err.contains("no device with an order: pair a relay first"),
        "{err}"
    );
    assert_eq!(measure_zone_count(port), 0);
}

#[test]
fn without_an_address_the_usage_names_the_mac_and_the_box_rule() {
    let (ok, _, err) = run(&[]);
    assert!(!ok);
    assert!(
        err.contains("usage: SOLITE_PASSWORD=<password> sh scripts/measure-api.sh"),
        "{err}"
    );
    assert!(
        err.contains("on the Mac: sh scripts/measure-api.sh 8080"),
        "{err}"
    );
    assert!(
        err.contains("once per milestone, inside sh scripts/box.sh saves"),
        "{err}"
    );
}

#[test]
fn with_no_api_answering_it_says_so() {
    let (ok, _, err) = run(&[&format!("127.0.0.1:{}", free_port())]);
    assert!(!ok);
    assert!(
        err.contains("measure-api: no API answers at 127.0.0.1:"),
        "{err}"
    );
}

/// What a test server saw and does.
struct Fake {
    /// The creation request (equipment and bindings, counted from 1) that is answered 503.
    busy_at: Option<usize>,
    /// Each answer waits this long.
    delay: Duration,
    /// "METHOD path" of every request received.
    seen: Mutex<Vec<String>>,
}

/// A test server for the script's calls: an empty list of zones, one device with a data and an
/// order, 201 for the creations, 204 for the deletions. Returns its port.
fn fake_api(fake: Arc<Fake>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let fake = Arc::clone(&fake);
            thread::spawn(move || answer(stream, &fake));
        }
    });
    port
}

fn answer(mut stream: TcpStream, fake: &Fake) {
    let mut raw = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let n = stream.read(&mut buffer).unwrap_or(0);
        if n == 0 {
            return;
        }
        raw.extend_from_slice(&buffer[..n]);
        let text = String::from_utf8_lossy(&raw).into_owned();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if body.len() >= length {
                break;
            }
        }
    }
    let head = String::from_utf8_lossy(&raw).into_owned();
    let mut words = head.lines().next().unwrap_or("").split(' ');
    let (method, path) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let creations;
    {
        let mut seen = fake.seen.lock().unwrap();
        seen.push(format!("{method} {path}"));
        creations = seen
            .iter()
            .filter(|s| s.starts_with("POST /api/v1/equipments"))
            .count();
    }
    thread::sleep(fake.delay);
    let (status, body) = match (method, path) {
        ("GET", "/api/v1/auth/status") => ("200 OK", r#"{"setupRequired":false}"#.to_string()),
        ("POST", "/api/v1/auth/login") => (
            "200 OK",
            r#"{"accessToken":"stub-token","refreshToken":"stub-refresh"}"#.to_string(),
        ),
        ("GET", "/api/v1/zones") => ("200 OK", "[]".to_string()),
        ("GET", "/api/v1/devices") => (
            "200 OK",
            r#"[{"id":"d1","data":[{"id":"data-1","key":"state"}],"orders":[{"id":"order-1","key":"state"}]}]"#.to_string(),
        ),
        ("POST", "/api/v1/zones") => ("201 Created", r#"{"id":"zone-1"}"#.to_string()),
        ("POST", _) if fake.busy_at == Some(creations) => {
            ("503 Service Unavailable", r#"{"error":"Busy"}"#.to_string())
        }
        ("POST", "/api/v1/equipments") => ("201 Created", r#"{"id":"eq-1"}"#.to_string()),
        ("POST", _) => ("201 Created", r#"{"id":"binding"}"#.to_string()),
        ("DELETE", _) => ("204 No Content", String::new()),
        _ => ("404 Not Found", r#"{"error":"Not found"}"#.to_string()),
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn fake(busy_at: Option<usize>, delay_ms: u64) -> (u16, Arc<Fake>) {
    let fake = Arc::new(Fake {
        busy_at,
        delay: Duration::from_millis(delay_ms),
        seen: Mutex::new(Vec::new()),
    });
    (fake_api(Arc::clone(&fake)), fake)
}

#[test]
fn a_503_busy_is_counted_the_first_bad_answer_is_shown_and_everything_is_deleted() {
    let (port, fake) = fake(Some(10), 0);
    let (ok, out, err) = run(&[&port.to_string()]);
    assert!(!ok, "{out}");
    assert!(out.contains("16 answered 201, 1 Busy"), "{out}");
    assert!(
        err.contains("first answer that was not 201: request 10: status 503"),
        "{err}"
    );
    assert!(err.contains("Busy"), "{err}");
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen.contains(&"DELETE /api/v1/equipments/eq-1".to_string()),
        "{seen:?}"
    );
    assert!(
        seen.contains(&"DELETE /api/v1/zones/zone-1".to_string()),
        "{seen:?}"
    );
}

#[test]
#[ignore = "slow (11 s, over the production limit of 10 s): sh scripts/validate.sh runs it"]
fn a_total_over_10_seconds_fails_and_prints_the_time() {
    // 17 answers of 650 ms: about 11 s.
    let (port, fake) = fake(None, 650);
    let (ok, out, err) = run(&[&port.to_string()]);
    assert!(!ok, "{out}");
    assert!(out.contains("17 answered 201, 0 Busy, total "), "{out}");
    assert!(err.contains("over 10 s"), "{err}");
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen.contains(&"DELETE /api/v1/zones/zone-1".to_string()),
        "{seen:?}"
    );
}

#[test]
fn without_the_password_it_says_so_and_exits_2_and_a_refused_login_is_exit_1() {
    let (_host, port, _data) = serving(GARAGE);
    let (status, _, err) = run_status(&[&port.to_string()], None);
    assert_eq!(status, Some(2));
    assert!(
        err.contains("measure-api: set SOLITE_PASSWORD (the password of the box)"),
        "{err}"
    );
    // A fresh host is set up by the first run, and the same password logs in the second.
    let (ok, out, err) = run(&[&port.to_string()]);
    assert!(ok, "{out}\n{err}");
    let (ok, out, err) = run(&[&port.to_string()]);
    assert!(ok, "{out}\n{err}");
    // A wrong password: the host's text, exit 1, the password never printed.
    let wrong = "wrong-password-9876";
    let (status, out, err) = run_status(&[&port.to_string()], Some(wrong));
    assert_eq!(status, Some(1));
    assert!(
        err.contains("login refused (401): Invalid credentials"),
        "{err}"
    );
    assert!(!err.contains(wrong) && !out.contains(wrong), "{err}{out}");
}
