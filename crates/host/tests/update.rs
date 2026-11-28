//! Updates through the real server (spec 032, R6, R7): a signed file streamed into the installation,
//! the refusals that close the connection, the probation at the next start. The update key is a
//! build-time input, so the tests that install build the host a second time, in a folder of their
//! own, with the public key of a pair made here (`--update-keygen`): no key is written in a file.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

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
    let answer = serving.upload(&sign(&bytes, "9.9.0"), true);
    assert_eq!(answer.status, 200, "{}", answer.body);
    assert_eq!(
        serde_json::from_str::<Value>(&answer.body).unwrap(),
        json!({"success": true, "message": "Installed 9.9.0, restarting"})
    );
    let boot = std::fs::read_to_string(data.join("update/boot.json")).unwrap();
    assert_eq!(boot.trim(), r#"{"boot":1,"pending":true}"#);
    assert_eq!(
        std::fs::read(data.join("update/slot-1.bin")).unwrap(),
        bytes
    );
    assert_eq!(
        wait_for_line(&mut serving, "written to slot"),
        "update: 9.9.0 written to slot 1; restart the host to put it on probation"
    );
    // Another file before the restart: refused, and nothing changes.
    let again = serving.upload(&sign(&bytes, "9.9.1"), true);
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
    let refused = restarted.upload(&sign(&bytes, "9.9.1"), true);
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
    let mut file = sign(&image(20_000), "9.9.0");
    file[128 + 10_000] ^= 1;
    let answer = serving.upload(&file, true);
    assert_eq!(answer.status, 400, "{}", answer.body);
    assert!(answer.body.contains("Image does not match its signature"));
    assert!(!data.join("update/boot.json").exists());
    assert!(!data.join("update/slot-1.bin").exists());
    assert!(!data.join("update/slot-1.bin.part").exists());
    // The state is idle again: the right file goes through.
    let good = serving.upload(&sign(&image(20_000), "9.9.0"), true);
    assert_eq!(good.status, 200, "{}", good.body);
}

#[test]
fn without_a_token_the_upload_is_401_the_connection_closes_and_the_image_is_not_answered() {
    let host = keyed_host();
    let data = temp("no-token");
    let serving = serve(&host.binary, &data);
    let answer = serving.upload(&sign(&image(64 * 1024), "9.9.0"), false);
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
fn the_host_answers_501_to_a_restart_and_401_without_a_token() {
    let data = temp("restart");
    let serving = serve(Path::new(plain_binary()), &data);
    let answer = serving.request(
        "POST",
        "/api/v1/system/restart",
        b"",
        "application/json",
        true,
    );
    assert_eq!(answer.status, 501, "{}", answer.body);
    assert_eq!(
        serde_json::from_str::<Value>(&answer.body).unwrap(),
        json!({"error": "The host does not restart itself"})
    );
    let answer = serving.request(
        "POST",
        "/api/v1/system/restart",
        b"",
        "application/json",
        false,
    );
    assert_eq!(answer.status, 401, "{}", answer.body);
}

#[test]
#[ignore = "slow: waits for the door's 30 s idle timeout"]
fn an_upload_that_stops_mid_image_ends_within_the_idle_timeout_and_the_next_one_is_not_409() {
    let host = keyed_host();
    let data = temp("stall");
    let serving = serve(&host.binary, &data);
    let file = sign(&image(40_000), "9.9.0");
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

// ---- A release pulled from a server (spec 039) ----

fn answer(status: u16, headers: &[String], body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", body.len()).into_bytes();
    for header in headers {
        out.extend_from_slice(format!("{header}\r\n").as_bytes());
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    out
}

fn digest_hex(bytes: &[u8]) -> String {
    solite_core::update::image_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A server of a release `version` whose file is `sup`, behind two redirects; its port.
fn serve_release(sup: &[u8], version: &str) -> u16 {
    // The listener is kept from its bind to its serving: the manifest names its own port, and no
    // other test can take the port in between.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let manifest = json!({
        "format": 1, "version": version, "minVersion": null,
        "image": {
            "size": sup.len(),
            "sha256": digest_hex(sup),
            "url": format!("http://127.0.0.1:{port}/a"),
        },
    });
    let routes = vec![
        ("/m.json", answer(200, &[], manifest.to_string().as_bytes())),
        (
            "/a",
            answer(302, &[format!("Location: http://127.0.0.1:{port}/b")], b""),
        ),
        ("/b", answer(302, &["Location: /c".to_string()], b"")),
        ("/c", answer(200, &[], sup)),
    ];
    thread::spawn(move || {
        for mut stream in listener.incoming().map_while(Result::ok) {
            let routes = routes.clone();
            thread::spawn(move || {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(1) => head.push(byte[0]),
                        _ => return,
                    }
                }
                let text = String::from_utf8_lossy(&head).into_owned();
                let path = text.split(' ').nth(1).unwrap_or("/");
                if let Some((_, a)) = routes.iter().find(|(p, _)| *p == path) {
                    let _ = stream.write_all(a);
                }
            });
        }
    });
    port
}

fn set_manifest_url(serving: &Serving, port: u16) {
    let body = json!({"update.manifestUrl": format!("http://127.0.0.1:{port}/m.json")});
    let answer = serving.request(
        "PUT",
        "/api/v1/settings",
        body.to_string().as_bytes(),
        "application/json",
        true,
    );
    assert_eq!(answer.status, 200, "{}", answer.body);
}

#[test]
fn a_check_then_an_empty_post_pulls_the_release_through_two_redirects() {
    let host = keyed_host();
    let data = temp("pull");
    let mut serving = serve(&host.binary, &data);
    let sup = sign(&image(40 * 1024), "9.9.0");
    let port = serve_release(&sup, "9.9.0");
    set_manifest_url(&serving, port);

    let check = serving.request(
        "POST",
        "/api/v1/system/version/check",
        &[],
        "text/plain",
        true,
    );
    assert_eq!(check.status, 200, "{}", check.body);
    let info: Value = serde_json::from_str(&check.body).unwrap();
    assert_eq!(info["latest"], "9.9.0");
    assert_eq!(info["updateAvailable"], true);
    let version = serving.request(
        "GET",
        "/api/v1/system/version",
        &[],
        "application/json",
        true,
    );
    assert_eq!(serde_json::from_str::<Value>(&version.body).unwrap(), info);

    let pull = serving.request(
        "POST",
        "/api/v1/system/update",
        &[],
        "application/json",
        true,
    );
    assert_eq!(pull.status, 200, "{}", pull.body);
    assert_eq!(
        serde_json::from_str::<Value>(&pull.body).unwrap()["message"],
        "Installed 9.9.0, restarting"
    );
    let line = wait_for_line(&mut serving, "written to slot");
    assert!(
        line.starts_with("update: 9.9.0 written to slot 1"),
        "{line}"
    );
    assert_eq!(
        std::fs::read(data.join("update/slot-1.bin")).unwrap(),
        sup[128..]
    );
}

#[test]
fn an_empty_post_without_a_content_length_is_the_pull_and_a_release_not_newer_is_400() {
    let host = keyed_host();
    let data = temp("pull-old");
    let serving = serve(&host.binary, &data);
    let sup = sign(&image(10 * 1024), "0.0.1");
    let port = serve_release(&sup, "0.0.1");
    set_manifest_url(&serving, port);
    // No `Content-Length`, no `Transfer-Encoding`: an empty body, so the pull (not the upload's 415).
    let head = format!(
        "POST /api/v1/system/update HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n\r\n",
        serving.port, serving.token
    );
    let pulled = serving.send_raw(head.as_bytes(), &[]);
    assert_eq!(pulled.status, 400, "{}", pulled.body);
    assert_eq!(
        serde_json::from_str::<Value>(&pulled.body).unwrap(),
        json!({"error": "No update available"})
    );
    assert!(!data.join("update/slot-1.bin").exists());
    // The upload form still wants its content type.
    let upload = serving.request(
        "POST",
        "/api/v1/system/update",
        b"x",
        "application/json",
        true,
    );
    assert_eq!(upload.status, 415);
}

#[test]
fn the_settings_of_the_release_check_are_accepted_and_checked() {
    let host = keyed_host();
    let data = temp("settings");
    let serving = serve(&host.binary, &data);
    let good =
        json!({"update.manifestUrl": "http://192.168.1.5:8000/m.json", "update.autoCheck": "true"});
    let put = serving.request(
        "PUT",
        "/api/v1/settings",
        good.to_string().as_bytes(),
        "application/json",
        true,
    );
    assert_eq!(put.status, 200, "{}", put.body);
    for (body, message) in [
        (
            json!({"update.autoCheck": "yes"}),
            "Invalid value for update.autoCheck",
        ),
        (
            json!({"update.manifestUrl": "http://example.com/m"}),
            "Invalid value for update.manifestUrl",
        ),
    ] {
        let put = serving.request(
            "PUT",
            "/api/v1/settings",
            body.to_string().as_bytes(),
            "application/json",
            true,
        );
        assert_eq!(put.status, 400);
        assert_eq!(
            serde_json::from_str::<Value>(&put.body).unwrap(),
            json!({"error": message})
        );
    }
    let get = serving.request("GET", "/api/v1/settings", &[], "application/json", true);
    let settings: Value = serde_json::from_str(&get.body).unwrap();
    assert_eq!(settings["update.autoCheck"], "true");
}

// ---- A cut download resumed (spec 040 R6) ----

/// What the server of `serve_cutting` does after it cut the file's first answer.
#[derive(Clone, Copy, PartialEq)]
enum Then {
    /// Honours the `Range`: a 206 with the rest.
    Resumes,
    /// Ignores the `Range`: a 200 with the whole file.
    IgnoresRange,
    /// Honours it, but with another `ETag`: the file changed.
    OtherEtag,
    /// Honours it, and cuts after 1000 bytes every time.
    CutsAgain,
}

/// A server of a release whose file is `sup` behind two redirects: its first answer for the file
/// stops after `at` bytes (the length announced is the whole file's); the ranged requests are
/// answered as `then` says. Returns the port and the `Range` headers seen, each as
/// `<path> <header>`.
fn serve_cutting(
    sup: &[u8],
    version: &str,
    at: usize,
    then: Then,
) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let manifest = json!({
        "format": 1, "version": version, "minVersion": null,
        "image": {
            "size": sup.len(),
            "sha256": digest_hex(sup),
            "url": format!("http://127.0.0.1:{port}/a"),
        },
    })
    .to_string();
    let sup = sup.to_vec();
    let ranges: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = Arc::clone(&ranges);
    thread::spawn(move || {
        for mut stream in listener.incoming().map_while(Result::ok) {
            let (sup, manifest, seen) = (sup.clone(), manifest.clone(), Arc::clone(&seen));
            thread::spawn(move || {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(1) => head.push(byte[0]),
                        _ => return,
                    }
                }
                let text = String::from_utf8_lossy(&head).into_owned();
                let path = text.split(' ').nth(1).unwrap_or("/").to_string();
                let range = text
                    .lines()
                    .find_map(|line| line.strip_prefix("Range: "))
                    .map(str::to_string);
                if let Some(range) = &range {
                    seen.lock().unwrap().push(format!("{path} {range}"));
                }
                let len = sup.len();
                let reply = match path.as_str() {
                    "/m.json" => answer(200, &[], manifest.as_bytes()),
                    "/a" => answer(302, &[format!("Location: http://127.0.0.1:{port}/b")], b""),
                    "/b" => answer(302, &["Location: /c".to_string()], b""),
                    "/c" => match range
                        .as_deref()
                        .and_then(|r| r.strip_prefix("bytes="))
                        .and_then(|r| r.strip_suffix('-'))
                        .and_then(|r| r.parse::<usize>().ok())
                    {
                        // The first answer: the whole length announced, `at` bytes sent.
                        None => {
                            let mut out = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nETag: \"v1\"\r\n\r\n"
                            )
                            .into_bytes();
                            out.extend_from_slice(&sup[..at]);
                            out
                        }
                        Some(_) if then == Then::IgnoresRange => answer(200, &[], &sup),
                        Some(from) => {
                            let etag = if then == Then::OtherEtag {
                                "\"v2\""
                            } else {
                                "\"v1\""
                            };
                            let end = if then == Then::CutsAgain {
                                (from + 1000).min(len)
                            } else {
                                len
                            };
                            let mut out = format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {from}-{}/{len}\r\nContent-Length: {}\r\nETag: {etag}\r\n\r\n",
                                len - 1,
                                len - from
                            )
                            .into_bytes();
                            out.extend_from_slice(&sup[from..end]);
                            out
                        }
                    },
                    _ => answer(404, &[], b""),
                };
                let _ = stream.write_all(&reply);
            });
        }
    });
    (port, ranges)
}

fn pull(serving: &Serving) -> (u16, Value) {
    let answer = serving.request(
        "POST",
        "/api/v1/system/update",
        &[],
        "application/json",
        true,
    );
    let body = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
    (answer.status, body)
}

fn no_slot(data: &Path) {
    assert!(!data.join("update/slot-1.bin").exists());
    assert!(!data.join("update/slot-1.bin.part").exists());
    assert!(!data.join("update/boot.json").exists());
}

#[test]
fn a_cut_download_is_resumed_through_the_redirects_and_installs() {
    let host = keyed_host();
    let data = temp("resume");
    let mut serving = serve(&host.binary, &data);
    let sup = sign(&image(40 * 1024), "9.9.0");
    let (port, ranges) = serve_cutting(&sup, "9.9.0", 15_000, Then::Resumes);
    set_manifest_url(&serving, port);
    let (status, body) = pull(&serving);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["message"], "Installed 9.9.0, restarting");
    // Every hop of the chain carried the range, from the byte read.
    assert_eq!(
        *ranges.lock().unwrap(),
        vec![
            "/a bytes=15000-".to_string(),
            "/b bytes=15000-".to_string(),
            "/c bytes=15000-".to_string()
        ]
    );
    let line = wait_for_line(&mut serving, "resuming (1/3)");
    assert!(
        line.contains(&format!("download cut at 15000 of {} bytes", sup.len())),
        "{line}"
    );
    assert_eq!(
        std::fs::read(data.join("update/slot-1.bin")).unwrap(),
        sup[128..]
    );
}

#[test]
fn a_server_without_range_support_fails_the_pull_cleanly() {
    let host = keyed_host();
    let data = temp("resume-norange");
    let serving = serve(&host.binary, &data);
    let sup = sign(&image(40 * 1024), "9.9.0");
    let (port, _) = serve_cutting(&sup, "9.9.0", 15_000, Then::IgnoresRange);
    set_manifest_url(&serving, port);
    let (status, body) = pull(&serving);
    assert_eq!(status, 500, "{body}");
    assert_eq!(
        body["error"],
        "Download failed: the server does not resume (200 to a range request)"
    );
    no_slot(&data);
}

#[test]
fn a_file_changed_between_the_two_requests_fails_the_pull_cleanly() {
    let host = keyed_host();
    let data = temp("resume-changed");
    let serving = serve(&host.binary, &data);
    let sup = sign(&image(40 * 1024), "9.9.0");
    let (port, _) = serve_cutting(&sup, "9.9.0", 15_000, Then::OtherEtag);
    set_manifest_url(&serving, port);
    let (status, body) = pull(&serving);
    assert_eq!(status, 500, "{body}");
    assert_eq!(
        body["error"],
        "Download failed: the file changed on the server"
    );
    no_slot(&data);
}

#[test]
#[ignore = "slow: waits the real 2 + 4 + 8 s of the resumes (the core tests prove the waits on a fake clock)"]
fn a_server_that_cuts_every_time_fails_after_three_resumes_waiting_2_4_and_8_seconds() {
    let host = keyed_host();
    let data = temp("resume-exhausted");
    let serving = serve(&host.binary, &data);
    let sup = sign(&image(40 * 1024), "9.9.0");
    let (port, ranges) = serve_cutting(&sup, "9.9.0", 15_000, Then::CutsAgain);
    set_manifest_url(&serving, port);
    let started = Instant::now();
    let (status, body) = pull(&serving);
    let elapsed = started.elapsed();
    assert_eq!(status, 500, "{body}");
    assert_eq!(
        body["error"],
        "Download failed: connection lost (after 3 resumes)"
    );
    // The waits are real seconds on the host (2 + 4 + 8): never shorter, whatever the machine's
    // speed (no upper bound is asserted).
    assert!(elapsed >= Duration::from_secs(14), "{elapsed:?}");
    // Three resumed requests, each from the byte the last one stopped at, over the three hops.
    let froms: Vec<String> = ranges
        .lock()
        .unwrap()
        .iter()
        .filter(|seen| seen.starts_with("/c "))
        .cloned()
        .collect();
    assert_eq!(
        froms,
        vec!["/c bytes=15000-", "/c bytes=16000-", "/c bytes=17000-"]
    );
    no_slot(&data);
}
