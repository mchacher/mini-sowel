//! `scripts/demo-garage.sh` sets the garage of the demos up through the API of a running host, and
//! refuses to run twice.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const GARAGE: &str = "../../scenarios/garage.json";
const SCRIPT: &str = "../../scripts/demo-garage.sh";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A host program that is killed and reaped when the test ends, however it ends.
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

/// Pairs the garage's three devices, then serves; returns once the host says `serving:`.
fn serving() -> (Reaped, u16, TempDir) {
    let dir = std::env::temp_dir().join(format!(
        "solite-demo-script-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let data = TempDir(dir);
    let output = host()
        .env("SOLITE_CLOSE_GRACE_SECONDS", "1")
        .args(["--sim", GARAGE, "--pair", "2", "--speed", "100", "--data"])
        .arg(&data.0)
        .output()
        .expect("the host program starts");
    assert!(output.status.success(), "{output:?}");
    let port = free_port();
    let mut child = Reaped(
        host()
            .args([
                "--sim",
                GARAGE,
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

fn run_script(port: u16) -> (bool, String, String) {
    let output = Command::new("sh")
        .args([SCRIPT, &port.to_string()])
        .output()
        .expect("sh runs the script");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn the_script_sets_the_garage_up_prints_the_ids_and_refuses_a_second_run() {
    let (_host, port, _data) = serving();
    let (ok, out, err) = run_script(port);
    assert!(ok, "{out}\n{err}");
    for expected in [
        "zone      ",
        "Garage climate",
        "Garage motion",
        "Garage light",
        "instance  ",
        "motion-light, timeout 20s",
    ] {
        assert!(out.contains(expected), "{expected:?} missing from:\n{out}");
    }
    // The zone, its three equipments and the recipe's instance exist.
    let get = |path: &str| {
        let output = Command::new("curl")
            .args(["-s", &format!("http://127.0.0.1:{port}/api/v1/{path}")])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    assert!(get("zones").contains("\"Garage\""));
    assert!(get("equipments").matches("\"Garage ").count() >= 3);
    assert!(get("recipe-instances").contains("motion-light"));

    let (ok, out, err) = run_script(port);
    assert!(!ok, "a second run must refuse: {out}");
    assert!(err.contains("a zone named Garage exists already"), "{err}");
    assert_eq!(get("zones").matches("\"Garage\"").count(), 1);
}

#[test]
fn the_script_says_so_when_no_host_answers() {
    let (ok, _, err) = run_script(free_port());
    assert!(!ok);
    assert!(err.contains("no host answers"), "{err}");
}

/// A host that answers like the real one until it refuses the creation of the first equipment
/// with a 500. Returns its port; the thread ends with the test process.
fn host_refusing_equipments() -> u16 {
    use std::io::{Read, Write};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buffer = [0u8; 4096];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let head = String::from_utf8_lossy(&buffer[..n]).into_owned();
            let line = head.lines().next().unwrap_or("").to_string();
            let (status, body) = if line.starts_with("GET /api/v1/zones") {
                ("200 OK", "[]".to_string())
            } else if line.starts_with("GET /api/v1/devices") {
                let devices: Vec<String> = ["TH01", "MS01", "ZBMINIR2"]
                    .iter()
                    .map(|m| format!(r#"{{"id":"d-{m}","model":"{m}"}}"#))
                    .collect();
                ("200 OK", format!("[{}]", devices.join(",")))
            } else if line.starts_with("POST /api/v1/zones") {
                ("200 OK", r#"{"id":"zone-123"}"#.to_string())
            } else if line.starts_with("PUT /api/v1/devices") {
                ("200 OK", "{}".to_string())
            } else {
                (
                    "500 Internal Server Error",
                    r#"{"error":"boom"}"#.to_string(),
                )
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

#[test]
fn a_refused_call_shows_the_http_error_and_the_command_that_undoes_the_zone() {
    let port = host_refusing_equipments();
    let (ok, _, err) = run_script(port);
    assert!(!ok);
    assert!(err.contains("500"), "the HTTP error must show: {err}");
    assert!(err.contains("zone-123"), "{err}");
    assert!(
        err.contains(&format!(
            "curl -sS -f -X DELETE http://127.0.0.1:{port}/api/v1/zones/zone-123"
        )),
        "{err}"
    );
}
