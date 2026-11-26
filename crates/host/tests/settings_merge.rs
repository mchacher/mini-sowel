//! The settings merge through the real server (spec 040, R5): a `PUT /api/v1/settings` that gives
//! only `update.manifestUrl` keeps the home's name and location, and the four keys are still there
//! after the host restarts on the same data folder (the file read back). Sowel's `upsert` changes
//! the keys it is given (the core's `a_put_keeps_the_keys_it_does_not_give`); this checks it end to
//! end. The plain test binary is enough: no update key is needed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

const GARAGE: &str = "../../scenarios/garage.json";
const PASSWORD: &str = "test-password-1234";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "solite-settings-merge-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A running `--serve` on the simulated garage, signed in.
struct Serving {
    child: Child,
    port: u16,
    lines: Receiver<String>,
    token: String,
}

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Serving {
    /// Stops the host as Ctrl-C does (the way a user restarts it), and waits for its end.
    fn stop(mut self) {
        let pid = self.child.id().to_string();
        let sent = Command::new("kill").args(["-INT", &pid]).status().unwrap();
        assert!(sent.success());
        let _ = self.child.wait();
    }
}

fn serve(data: &Path) -> Serving {
    for _ in 0..5 {
        if let Some(serving) = try_serve(data) {
            return serving;
        }
    }
    panic!("the host could not listen on a free port in five tries");
}

fn try_serve(data: &Path) -> Option<Serving> {
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_solite-host"))
        .args([
            "--sim",
            GARAGE,
            "--serve",
            &port.to_string(),
            "--speed",
            "10",
            "--data",
        ])
        .arg(data)
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
        token: String::new(),
    };
    let mut shown = Vec::new();
    loop {
        match serving.lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => {
                let ready = line.starts_with("serving:");
                shown.push(line);
                if ready {
                    serving.sign_in();
                    return Some(serving);
                }
            }
            Err(error) => {
                let _ = serving.child.kill();
                let _ = serving.child.wait();
                let mut stderr = String::new();
                if let Some(mut pipe) = serving.child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                if error == mpsc::RecvTimeoutError::Disconnected
                    && stderr.starts_with("serve: cannot listen on ")
                {
                    return None;
                }
                panic!("the host stopped or stayed silent: {shown:?}, {stderr}");
            }
        }
    }
}

impl Serving {
    fn sign_in(&mut self) {
        let (_, body) = self.request("GET", "/api/v1/auth/status", b"", false);
        let fresh = serde_json::from_str::<Value>(&body).unwrap()["setupRequired"] == true;
        let (route, payload) = if fresh {
            (
                "setup",
                json!({"username": "admin", "password": PASSWORD, "displayName": "Admin"}),
            )
        } else {
            ("login", json!({"username": "admin", "password": PASSWORD}))
        };
        let (status, body) = self.request(
            "POST",
            &format!("/api/v1/auth/{route}"),
            payload.to_string().as_bytes(),
            false,
        );
        assert!(status == 200 || status == 201, "{body}");
        self.token = serde_json::from_str::<Value>(&body).unwrap()["accessToken"]
            .as_str()
            .unwrap()
            .to_string();
    }

    /// One request, the connection read to its end: the status and the body.
    fn request(&self, method: &str, path: &str, body: &[u8], with_token: bool) -> (u16, String) {
        let auth = if with_token {
            format!("Authorization: Bearer {}\r\n", self.token)
        } else {
            String::new()
        };
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\n\r\n",
            self.port,
            body.len()
        );
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP answer");
        (
            head.split(' ').nth(1).unwrap().parse().unwrap(),
            body.to_string(),
        )
    }

    fn put(&self, settings: &Value) {
        let (status, body) = self.request(
            "PUT",
            "/api/v1/settings",
            settings.to_string().as_bytes(),
            true,
        );
        assert_eq!(status, 200, "{body}");
    }

    fn settings(&self) -> Value {
        let (status, body) = self.request("GET", "/api/v1/settings", b"", true);
        assert_eq!(status, 200, "{body}");
        serde_json::from_str(&body).unwrap()
    }
}

#[test]
fn a_put_of_the_manifest_address_alone_keeps_the_home_and_survives_a_restart() {
    let data = temp("merge");
    let serving = serve(&data);
    serving.put(&json!({
        "home.name": "Annexe",
        "home.latitude": "45.76",
        "home.longitude": "4.83"
    }));
    serving.put(&json!({"update.manifestUrl": "https://example.org/manifest.json"}));
    let expected = json!({
        "home.name": "Annexe",
        "home.latitude": "45.76",
        "home.longitude": "4.83",
        "update.manifestUrl": "https://example.org/manifest.json"
    });
    assert_eq!(serving.settings(), expected);

    // The host restarts on the same data folder: the file is read back, the four keys are there.
    serving.stop();
    let restarted = serve(&data);
    assert_eq!(restarted.settings(), expected);
}
