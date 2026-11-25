//! Updates through the real server (spec 032, R6, R7): a signed file streamed into the installation,
//! the refusals that close the connection, the probation at the next start. The update key is a
//! build-time input, so the tests that install build the host a second time, in a folder of their
//! own, with the public key of a pair made here (`--update-keygen`): no key is written in a file.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
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
        "solite-update-e2e-{name}-{}-{}",
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

/// A host program that has a key: the pair, and the host built with its public key.
struct KeyedHost {
    binary: PathBuf,
    key_file: PathBuf,
    public: String,
}

/// The binary of this test run, built without a key unless the environment gave one.
fn plain_binary() -> &'static str {
    env!("CARGO_BIN_EXE_solite-host")
}

fn keyed_host() -> &'static KeyedHost {
    static HOST: OnceLock<KeyedHost> = OnceLock::new();
    HOST.get_or_init(|| {
        let dir = temp("key");
        let key_file = dir.join("signing.update-key");
        let output = Command::new(plain_binary())
            .arg("--update-keygen")
            .arg(&key_file)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        let public = text
            .lines()
            .find_map(|l| l.strip_prefix("public key: "))
            .expect("the public key line")
            .to_string();
        // The same program, built again with that key: in its own target folder, kept between
        // runs (only the host crate is compiled again).
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("update-host-build");
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let built = Command::new(cargo)
            .args(["build", "-p", "solite-host", "--quiet", "--manifest-path"])
            .arg(&manifest)
            .arg("--target-dir")
            .arg(&target)
            .env("SOLITE_UPDATE_PUBLIC_KEY", &public)
            .status()
            .unwrap();
        assert!(built.success(), "the host with an update key did not build");
        KeyedHost {
            binary: target.join("debug/solite-host"),
            key_file,
            public,
        }
    })
}

/// A byte string that is not all the same.
fn image(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 13 + i / 97) % 256).unwrap())
        .collect()
}

/// Signs `image` as `version` with the keyed host's key, through `--update-sign`.
fn sign(image_bytes: &[u8], version: &str) -> Vec<u8> {
    let host = keyed_host();
    let dir = temp("sign");
    std::fs::write(dir.join("image.bin"), image_bytes).unwrap();
    let output = Command::new(plain_binary())
        .arg("--update-sign")
        .arg(dir.join("image.bin"))
        .args(["--version", version, "--key"])
        .arg(&host.key_file)
        .arg("--out")
        .arg(dir.join("image.sup"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    std::fs::read(dir.join("image.sup")).unwrap()
}

/// A running `--serve` on the simulated garage.
struct Serving {
    child: Child,
    port: u16,
    lines: Receiver<String>,
    shown: Vec<String>,
    token: String,
}

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(binary: &Path, data: &Path) -> Serving {
    for _ in 0..5 {
        if let Some(serving) = try_serve(binary, data) {
            return serving;
        }
    }
    panic!("the host could not listen on a free port in five tries");
}

fn try_serve(binary: &Path, data: &Path) -> Option<Serving> {
    let port = free_port();
    let mut child = Command::new(binary)
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
        shown: Vec::new(),
        token: String::new(),
    };
    loop {
        match serving.lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => {
                let ready = line.starts_with("serving:");
                serving.shown.push(line);
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
                panic!(
                    "the host stopped or stayed silent: {:?}, {stderr}",
                    serving.shown
                );
            }
        }
    }
}

/// One answer: status, body. `tiny_http` does not let a program set the
/// `Connection` header of its answer: the door (`front.rs`) tells it `Connection: close` for every
/// request, so the connection is closed after the answer, which these tests read to its end.
struct Answer {
    status: u16,
    body: String,
    /// How many `HTTP/1.1` status lines the connection carried.
    answers: usize,
}

impl Serving {
    fn sign_in(&mut self) {
        let answer = self.request("GET", "/api/v1/auth/status", &[], "application/json", false);
        let fresh = serde_json::from_str::<Value>(&answer.body).unwrap()["setupRequired"] == true;
        let (route, payload) = if fresh {
            (
                "setup",
                json!({"username": "admin", "password": PASSWORD, "displayName": "Admin"}),
            )
        } else {
            ("login", json!({"username": "admin", "password": PASSWORD}))
        };
        let answer = self.request(
            "POST",
            &format!("/api/v1/auth/{route}"),
            payload.to_string().as_bytes(),
            "application/json",
            false,
        );
        assert!(
            answer.status == 200 || answer.status == 201,
            "{}",
            answer.body
        );
        self.token = serde_json::from_str::<Value>(&answer.body).unwrap()["accessToken"]
            .as_str()
            .unwrap()
            .to_string();
    }

    /// A request with the token (or without), the body as given; the connection is read to its
    /// end, so what the server sends after its answer is counted.
    fn request(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        content_type: &str,
        with_token: bool,
    ) -> Answer {
        let auth = if with_token && !self.token.is_empty() {
            format!("Authorization: Bearer {}\r\n", self.token)
        } else {
            String::new()
        };
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: {content_type}\r\n{auth}Content-Length: {}\r\n\r\n",
            self.port,
            body.len()
        );
        self.send_raw(head.as_bytes(), body)
    }

    fn send_raw(&self, head: &[u8], body: &[u8]) -> Answer {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        stream.write_all(head).unwrap();
        // The server may answer and close before the whole body is sent: a failed write is fine.
        let _ = stream.write_all(body);
        read_answer(&mut stream)
    }

    fn upload(&self, file: &[u8], with_token: bool) -> Answer {
        self.request(
            "POST",
            "/api/v1/system/update",
            file,
            "application/octet-stream",
            with_token,
        )
    }
}

fn read_answer(stream: &mut TcpStream) -> Answer {
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP answer");
    Answer {
        status: head.split(' ').nth(1).unwrap().parse().unwrap(),
        body: body.to_string(),
        answers: text.matches("HTTP/1.1 ").count(),
    }
}

fn wait_for_line(serving: &mut Serving, wanted: &str) -> String {
    if let Some(line) = serving.shown.iter().find(|l| l.contains(wanted)) {
        return line.clone();
    }
    loop {
        let line = serving
            .lines
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("no line with {wanted:?}: {:?}", serving.shown));
        serving.shown.push(line.clone());
        if line.contains(wanted) {
            return line;
        }
    }
}

#[test]
fn a_signed_file_of_64_kb_is_streamed_installed_and_booted_on_probation() {
    let host = keyed_host();
    let data = temp("install");
    let mut serving = serve(&host.binary, &data);
    assert!(
        serving
            .shown
            .contains(&format!("update: key {}", &host.public[..8])),
        "{:?}",
        serving.shown
    );
    let bytes = image(64 * 1024);
    let answer = serving.upload(&sign(&bytes, "0.2.0"), true);
    assert_eq!(answer.status, 200, "{}", answer.body);
    assert_eq!(
        serde_json::from_str::<Value>(&answer.body).unwrap(),
        json!({"success": true, "message": "Installed 0.2.0, restarting"})
    );
    let boot = std::fs::read_to_string(data.join("update/boot.json")).unwrap();
    assert_eq!(boot.trim(), r#"{"boot":1,"pending":true}"#);
    assert_eq!(
        std::fs::read(data.join("update/slot-1.bin")).unwrap(),
        bytes
    );
    assert_eq!(
        wait_for_line(&mut serving, "written to slot"),
        "update: 0.2.0 written to slot 1; restart the host to put it on probation"
    );
    // Another file before the restart: refused, and nothing changes.
    let again = serving.upload(&sign(&bytes, "0.3.0"), true);
    assert_eq!(again.status, 409, "{}", again.body);
    assert!(
        again
            .body
            .contains("An update is installed and waits for the restart")
    );
    assert_eq!(again.answers, 1);
    assert_eq!(
        std::fs::read_to_string(data.join("update/boot.json")).unwrap(),
        boot
    );
    // After a restart the host is on probation, and refuses an installation until confirmed.
    drop(serving);
    let restarted = serve(&host.binary, &data);
    assert!(
        restarted
            .shown
            .iter()
            .any(|l| l == "update: slot 1 on probation"),
        "{:?}",
        restarted.shown
    );
    let refused = restarted.upload(&sign(&bytes, "0.3.0"), true);
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(
        refused
            .body
            .contains("The running image is not confirmed yet")
    );
}

#[test]
fn a_file_with_one_changed_byte_is_refused_and_leaves_nothing_in_the_slot() {
    let host = keyed_host();
    let data = temp("altered");
    let serving = serve(&host.binary, &data);
    let mut file = sign(&image(20_000), "0.2.0");
    file[128 + 10_000] ^= 1;
    let answer = serving.upload(&file, true);
    assert_eq!(answer.status, 400, "{}", answer.body);
    assert!(answer.body.contains("Image does not match its signature"));
    assert!(!data.join("update/boot.json").exists());
    assert!(!data.join("update/slot-1.bin").exists());
    assert!(!data.join("update/slot-1.bin.part").exists());
    // The state is idle again: the right file goes through.
    let good = serving.upload(&sign(&image(20_000), "0.2.0"), true);
    assert_eq!(good.status, 200, "{}", good.body);
}

#[test]
fn without_a_token_the_upload_is_401_the_connection_closes_and_the_image_is_not_answered() {
    let host = keyed_host();
    let data = temp("no-token");
    let serving = serve(&host.binary, &data);
    let answer = serving.upload(&sign(&image(64 * 1024), "0.2.0"), false);
    assert_eq!(answer.status, 401, "{}", answer.body);
    assert_eq!(
        answer.answers, 1,
        "the image's bytes were read as another request"
    );
    assert!(!data.join("update").exists());
}

#[test]
fn the_refusals_of_the_route_close_the_connection_too() {
    let host = keyed_host();
    let data = temp("refusals");
    let serving = serve(&host.binary, &data);
    let body = vec![7u8; 2000];
    // Another content type: 415. Another method: 405. A length above the most: 413 before any read.
    let json_type = serving.request(
        "POST",
        "/api/v1/system/update",
        &body,
        "application/json",
        true,
    );
    let method = serving.request(
        "PUT",
        "/api/v1/system/update",
        &body,
        "application/json",
        true,
    );
    let too_long = serving.send_raw(
        format!(
            "POST /api/v1/system/update HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/octet-stream\r\nAuthorization: Bearer {}\r\nContent-Length: 5000000000\r\n\r\n",
            serving.port, serving.token
        )
        .as_bytes(),
        &[],
    );
    for (answer, status) in [(&json_type, 415), (&method, 405), (&too_long, 413)] {
        assert_eq!(answer.status, status, "{}", answer.body);
        assert_eq!(answer.answers, 1);
    }
    let version = serving.request(
        "GET",
        "/api/v1/system/version",
        &[],
        "application/json",
        true,
    );
    assert_eq!(version.status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&version.body).unwrap(),
        json!({"current": solite_version(), "latest": null, "updateAvailable": false, "releaseUrl": null})
    );
}

fn solite_version() -> &'static str {
    solite_core::VERSION
}

#[test]
fn a_host_built_without_a_key_says_so_at_start_and_answers_503() {
    if option_env!("SOLITE_UPDATE_PUBLIC_KEY").is_some() {
        return; // this run's own binary has a key: the keyless case is not in this build
    }
    let data = temp("no-key");
    let serving = serve(Path::new(plain_binary()), &data);
    assert!(
        serving
            .shown
            .iter()
            .any(|l| l == "update: no key, updates refused"),
        "{:?}",
        serving.shown
    );
    let answer = serving.upload(&[0u8; 300], true);
    assert_eq!(answer.status, 503, "{}", answer.body);
    assert!(
        answer
            .body
            .contains("Updates are not configured: this build has no update key")
    );
    assert_eq!(answer.answers, 1);
}

#[test]
#[ignore = "slow: waits for the door's 30 s idle timeout"]
fn an_upload_that_stops_mid_image_ends_within_the_idle_timeout_and_the_next_one_is_not_409() {
    let host = keyed_host();
    let data = temp("stall");
    let serving = serve(&host.binary, &data);
    let file = sign(&image(40_000), "0.2.0");
    let head = format!(
        "POST /api/v1/system/update HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/octet-stream\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\n\r\n",
        serving.port,
        serving.token,
        file.len()
    );
    let mut stream = TcpStream::connect(("127.0.0.1", serving.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(&file[..20_000]).unwrap();
    // Silence: the door ends the stream after 30 s without a byte, the installation aborts.
    let answer = read_answer(&mut stream);
    assert_eq!(answer.status, 400, "{}", answer.body);
    assert!(answer.body.contains("Truncated update"));
    assert!(!data.join("update/slot-1.bin.part").exists());
    let next = serving.upload(&file, true);
    assert_eq!(next.status, 200, "{}", next.body);
}
