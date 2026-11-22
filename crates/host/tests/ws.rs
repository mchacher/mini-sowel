//! Live events over `/ws` (spec 013) on the simulated radio: the upgrade and what is refused
//! around it, the origin rule, the four-client bound, slow and silent clients, the frames,
//! Ctrl-C, and the acceptance demo (the garage and its motion-light, seen from a WebSocket).
//! The client is the `tungstenite` crate's, over a socket that is set up by hand.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{Role, WebSocket};
use tungstenite::{Bytes, Error, Message};

const GARAGE: &str = "../../scenarios/garage.json";
const WELCOME: &str =
    r#"{"type":"connected","message":"Connected to Sowel engine","version":"0.1.0"}"#;
/// RFC 6455's example key, and the accept key it gives.
const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
const ACCEPT: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A data folder that is removed when the test ends.
struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn data_dir() -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "solite-ws-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    TempDir(dir)
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn host() -> Command {
    Command::new(env!("CARGO_BIN_EXE_solite-host"))
}

/// The pairing time of the tests, in seconds. The pairing window runs in real time (the speed
/// scales the scenario, not the window): at speed 100, eight seconds play 13 minutes 20 of the
/// scenario, so the whole garage (its last report at 10 minutes, its last motion cleared at
/// 4 minutes) is played and stored, as a longer window would leave it.
const PAIR_SECONDS: &str = "8";

/// `--pair` on the garage scenario. The pairing runs once for the whole file (it takes
/// `PAIR_SECONDS` of real time); each test gets a copy of the folder it wrote, the same files a
/// pairing of its own would leave.
fn pair(dir: &Path) {
    static PAIRED: OnceLock<PathBuf> = OnceLock::new();
    let paired = PAIRED.get_or_init(|| {
        let paired = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("ws-paired-garage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&paired);
        pair_once(&paired);
        paired
    });
    std::fs::create_dir_all(dir).unwrap();
    for entry in std::fs::read_dir(paired).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }
}

fn pair_once(dir: &Path) {
    let output = host()
        .args([
            "--sim",
            GARAGE,
            "--pair",
            PAIR_SECONDS,
            "--speed",
            "100",
            "--data",
        ])
        .arg(dir)
        .output()
        .expect("the host program starts");
    assert!(output.status.success(), "{output:?}");
}

/// A running `--serve`.
struct Serving {
    child: Reaped,
    port: u16,
    started: Instant,
    _lines: Receiver<String>,
    /// The data folder, removed after the host is killed.
    dir: Option<TempDir>,
}

/// Starts `--serve` on a free port. The port is chosen before the host binds it, so another
/// process can take it in between: a host that exits before it serves is started again.
fn serve(dir: &Path, speed: &str, env: &[(&str, &str)]) -> Serving {
    for _ in 0..5 {
        if let Some(serving) = try_serve(dir, speed, env) {
            return serving;
        }
    }
    panic!("the host could not bind a free port in five tries");
}

fn try_serve(dir: &Path, speed: &str, env: &[(&str, &str)]) -> Option<Serving> {
    let port = free_port();
    let mut command = host();
    command
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = Reaped(command.spawn().expect("the host program starts"));
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stderr = BufReader::new(child.stderr.take().unwrap());
    let (sender, lines) = mpsc::channel();
    let errors = sender.clone();
    thread::spawn(move || {
        for line in stderr.lines().map_while(Result::ok) {
            if errors.send(line).is_err() {
                break;
            }
        }
    });
    thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let mut shown = Vec::new();
    loop {
        match lines.recv_timeout(Duration::from_secs(20)) {
            Ok(line) => {
                let ready = line.starts_with("serving:");
                shown.push(line);
                if ready {
                    break;
                }
            }
            // The host ended without serving (a taken port): the caller tries again.
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!("the host stayed silent: {shown:?}");
            }
        }
    }
    Some(Serving {
        child,
        port,
        started: Instant::now(),
        _lines: lines,
        dir: None,
    })
}

impl Serving {
    /// Sends a request; (status, head, body).
    fn http(&self, method: &str, path: &str, body: &Value) -> (u16, String, String) {
        let text = if body.is_null() {
            String::new()
        } else {
            body.to_string()
        };
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            self.port,
            text.len()
        )
        .unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        let (head, body) = answer.split_once("\r\n\r\n").expect("an HTTP answer");
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, head.to_string(), body.to_string())
    }

    fn send(&self, method: &str, path: &str, body: &Value) -> (u16, Value) {
        let (status, head, body) = self.http(method, path, body);
        let body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            dechunk(&body)
        } else {
            body
        };
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    fn zone(&self, name: &str) -> String {
        let (status, zone) = self.send("POST", "/api/v1/zones", &json!({"name": name}));
        assert_eq!(status, 201, "{zone}");
        zone["id"].as_str().unwrap().to_string()
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

/// A host program that is killed and reaped when the test ends, however it ends.
struct Reaped(Child);

impl std::ops::Deref for Reaped {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl std::ops::DerefMut for Reaped {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

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

// ---- The client ----

/// The first line and the head of the answer to an upgrade request.
struct Answer {
    status: u16,
    head: String,
    body: String,
}

impl Answer {
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|line| {
            let (n, v) = line.split_once(':')?;
            n.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_string())
        })
    }
}

/// The request of an upgrade, with these extra headers; `Host` and the rest are the ones of a
/// browser unless the extras say otherwise.
fn upgrade_request(port: u16, path: &str, method: &str, extra: &[(&str, &str)]) -> String {
    let mut headers: Vec<(String, String)> = vec![
        ("Host".into(), format!("127.0.0.1:{port}")),
        ("Upgrade".into(), "websocket".into()),
        ("Connection".into(), "Upgrade".into()),
        ("Sec-WebSocket-Key".into(), KEY.into()),
        ("Sec-WebSocket-Version".into(), "13".into()),
    ];
    for (name, value) in extra {
        headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        headers.push(((*name).into(), (*value).into()));
    }
    let mut text = format!("{method} {path} HTTP/1.1\r\n");
    for (name, value) in headers {
        if !value.is_empty() {
            text.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    text.push_str("\r\n");
    text
}

/// Sends the request on a new connection and reads the head of the answer; the leftover bytes
/// (frames that came with it) and the stream.
fn open(port: u16, request: &str) -> (Answer, TcpStream, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let end = loop {
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break buffer.len(),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    };
    let rest = buffer.split_off(end);
    let head = String::from_utf8_lossy(&buffer).to_string();
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut answer = Answer {
        status,
        head,
        body: String::new(),
    };
    // A refusal has a body of Content-Length bytes.
    if status != 101 {
        let mut body = String::from_utf8_lossy(&rest).to_string();
        let _ = stream.read_to_string(&mut body);
        answer.body = body;
    }
    (
        answer,
        stream,
        if status == 101 { rest } else { Vec::new() },
    )
}

/// A WebSocket client of the host, upgraded.
struct Client {
    ws: WebSocket<TcpStream>,
}

fn connect_with(port: u16, extra: &[(&str, &str)]) -> (Answer, Client) {
    let (answer, stream, rest) = open(port, &upgrade_request(port, "/ws", "GET", extra));
    assert_eq!(answer.status, 101, "{}", answer.head);
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let ws = if rest.is_empty() {
        WebSocket::from_raw_socket(stream, Role::Client, None)
    } else {
        WebSocket::from_partially_read(stream, rest, Role::Client, None)
    };
    (answer, Client { ws })
}

fn connect(port: u16) -> Client {
    connect_with(port, &[]).1
}

fn is_timeout(error: &Error) -> bool {
    matches!(error, Error::Io(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

impl Client {
    /// The next message within `wait`, `None` when nothing came; a closed connection is an
    /// error.
    fn next(&mut self, wait: Duration) -> Result<Option<Message>, Error> {
        let end = Instant::now() + wait;
        loop {
            match self.ws.read() {
                Ok(message) => return Ok(Some(message)),
                Err(e) if is_timeout(&e) => {
                    if Instant::now() >= end {
                        return Ok(None);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// The next text message within `wait`, pings and pongs skipped.
    fn text(&mut self, wait: Duration) -> Option<String> {
        let end = Instant::now() + wait;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.next(left) {
                Ok(Some(Message::Text(text))) => return Some(text.as_str().to_string()),
                Ok(Some(Message::Ping(_) | Message::Pong(_))) => {}
                Ok(Some(other)) => panic!("unexpected {other:?}"),
                Ok(None) | Err(_) => return None,
            }
        }
    }

    fn welcome(&mut self) {
        assert_eq!(self.text(Duration::from_secs(5)).as_deref(), Some(WELCOME));
    }

    fn subscribe(&mut self, topics: &[&str]) {
        let message = json!({"type": "subscribe", "topics": topics}).to_string();
        self.ws.send(Message::Text(message.into())).unwrap();
    }

    /// The events of the next array within `wait`.
    fn events(&mut self, wait: Duration) -> Vec<Value> {
        match self.text(wait) {
            None => Vec::new(),
            Some(text) => serde_json::from_str::<Value>(&text)
                .unwrap()
                .as_array()
                .expect("an array")
                .clone(),
        }
    }

    /// Reads until an event of this type shows up in an array (within `wait`).
    fn event_of(&mut self, kind: &str, wait: Duration) -> Option<Value> {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            let left = end.saturating_duration_since(Instant::now());
            if let Some(found) = self
                .events(left.min(Duration::from_secs(1)))
                .into_iter()
                .find(|e| e["type"] == kind)
            {
                return Some(found);
            }
        }
        None
    }

    /// The close frame the host sends, within `wait`.
    fn closed_with(&mut self, wait: Duration) -> Option<(u16, String)> {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            match self.next(end.saturating_duration_since(Instant::now())) {
                Ok(Some(Message::Close(Some(frame)))) => {
                    return Some((u16::from(frame.code), frame.reason.as_str().to_string()));
                }
                Ok(Some(Message::Close(None))) => return Some((1005, String::new())),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }
}

fn started() -> Serving {
    let dir = data_dir();
    pair(&dir);
    let mut serving = serve(&dir, "100", &[]);
    serving.dir = Some(dir);
    serving
}

// ---- Tests ----

#[test]
fn a_client_gets_the_subprotocol_echoed_the_right_accept_key_and_the_welcome_first() {
    let serving = started();
    let (answer, mut client) =
        connect_with(serving.port, &[("Sec-WebSocket-Protocol", "bearer.x")]);
    assert_eq!(
        answer.header("Sec-WebSocket-Protocol").as_deref(),
        Some("bearer.x")
    );
    assert_eq!(
        answer.header("Sec-WebSocket-Accept").as_deref(),
        Some(ACCEPT)
    );
    assert_eq!(
        answer
            .header("Upgrade")
            .as_deref()
            .map(str::to_lowercase)
            .as_deref(),
        Some("websocket")
    );
    // The welcome is one object, not an array, and comes first.
    let first = client.text(Duration::from_secs(5)).unwrap();
    assert_eq!(first, WELCOME);
    assert!(serde_json::from_str::<Value>(&first).unwrap().is_object());
    drop(client);
    // No subprotocol offered: none echoed. Several: the first with `bearer.`.
    let (answer, _client) = connect_with(serving.port, &[]);
    assert_eq!(answer.header("Sec-WebSocket-Protocol"), None);
    let (answer, _client) = connect_with(
        serving.port,
        &[("Sec-WebSocket-Protocol", "foo, bearer.x, bearer.y")],
    );
    assert_eq!(
        answer.header("Sec-WebSocket-Protocol").as_deref(),
        Some("bearer.x")
    );
    let (answer, _client) = connect_with(serving.port, &[("Sec-WebSocket-Protocol", "foo")]);
    assert_eq!(answer.header("Sec-WebSocket-Protocol"), None);
}

#[test]
fn a_subscribed_client_gets_arrays_of_events_within_a_moment_and_one_that_is_not_gets_none() {
    let serving = started();
    let mut watcher = connect(serving.port);
    watcher.welcome();
    watcher.subscribe(&["zones"]);
    let mut deaf = connect(serving.port);
    deaf.welcome();
    deaf.subscribe(&["modes"]);
    thread::sleep(Duration::from_millis(400));
    let sent = Instant::now();
    let zone = serving.zone("Garage");
    let events = watcher.events(Duration::from_secs(3));
    assert!(
        sent.elapsed() < Duration::from_millis(1500),
        "{:?}",
        sent.elapsed()
    );
    assert!(
        events
            .iter()
            .any(|e| e["type"] == "zone.created" && e["zone"]["id"] == zone.as_str()),
        "{events:?}"
    );
    assert_eq!(deaf.text(Duration::from_millis(800)), None);
}

#[test]
#[ignore = "slow (a minute of the garage at speed 10): sh scripts/validate.sh runs it"]
fn the_garage_and_its_motion_light_are_seen_live_as_the_scenario_plays() {
    let dir = data_dir();
    pair(&dir);
    // Spec 010's garage set up, then spec 012's instance (timeout 20 s), then the host again.
    let first = serve(&dir, "100", &[]);
    let (_, devices) = first.send("GET", "/api/v1/devices", &Value::Null);
    let devices = devices.as_array().unwrap().clone();
    let zone = first.zone("Garage");
    let model = |m: &str| {
        devices.iter().find(|d| d["model"] == m).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let mut light = String::new();
    for (name, kind, m) in [
        ("Garage climate", "sensor", "TH01"),
        ("Garage motion", "sensor", "MS01"),
        ("Garage light", "light_onoff", "ZBMINIR2"),
    ] {
        let id = model(m);
        let (status, _) = first.send(
            "PUT",
            &format!("/api/v1/devices/{id}"),
            &json!({"zoneId": zone}),
        );
        assert_eq!(status, 200);
        let (status, equipment) = first.send(
            "POST",
            "/api/v1/equipments",
            &json!({"name": name, "type": kind, "zoneId": zone, "deviceIds": [id]}),
        );
        assert_eq!(status, 201);
        if m == "ZBMINIR2" {
            light = equipment["id"].as_str().unwrap().to_string();
        }
    }
    let (status, instance) = first.send(
        "POST",
        "/api/v1/recipe-instances",
        &json!({"recipeId": "motion-light", "params": {"zone": zone, "lights": [light], "timeout": "20s"}}),
    );
    assert_eq!(status, 201, "{instance}");
    let instance = instance["id"].as_str().unwrap().to_string();
    assert_eq!(first.interrupt(), Some(130));

    let serving = serve(&dir, "10", &[]);
    let mut client = connect(serving.port);
    client.welcome();
    client.subscribe(&["devices", "equipments", "zones"]);
    // Everything the garage says for a minute, with the time it was heard after the start.
    let mut heard: Vec<(f64, Value)> = Vec::new();
    while serving.started.elapsed() < Duration::from_secs(56) {
        for event in client.events(Duration::from_secs(1)) {
            heard.push((serving.started.elapsed().as_secs_f64(), event));
        }
    }
    let of = |kind: &str| -> Vec<&(f64, Value)> {
        heard.iter().filter(|(_, e)| e["type"] == kind).collect()
    };
    let device_data = |key: &str| -> Vec<&(f64, Value)> {
        of("device.data.updated")
            .into_iter()
            .filter(|(_, e)| e["key"] == key)
            .collect()
    };
    assert!(!device_data("temperature").is_empty(), "no temperature");
    assert!(!device_data("occupancy").is_empty(), "no occupancy");
    let zone_events: Vec<_> = of("zone.data.changed")
        .into_iter()
        .filter(|(_, e)| e["zoneId"] == zone.as_str())
        .collect();
    assert!(
        !zone_events.is_empty(),
        "no zone.data.changed of the garage"
    );
    assert!(
        zone_events
            .iter()
            .any(|(_, e)| e["aggregatedData"]["motion"] == true)
    );
    // The light: `state` true at about 6 s, false at about 44 s.
    let state: Vec<(f64, bool)> = of("equipment.data.changed")
        .into_iter()
        .filter(|(_, e)| e["equipmentId"] == light.as_str() && e["alias"] == "state")
        .map(|(t, e)| (*t, e["value"] == true))
        .collect();
    let on = state.iter().find(|(_, v)| *v).expect("the light came on").0;
    let off = state
        .iter()
        .find(|(t, v)| !*v && *t > on)
        .expect("the light went off")
        .0;
    assert!((3.0..14.0).contains(&on), "on at {on}: {state:?}");
    assert!((38.0..56.0).contains(&off), "off at {off}: {state:?}");
    // The recipe's orders, with their source.
    let executed: Vec<_> = of("equipment.order.executed")
        .into_iter()
        .filter(|(_, e)| e["equipmentId"] == light.as_str() && e["value"] == "ON")
        .collect();
    assert!(!executed.is_empty(), "no order executed");
    assert_eq!(
        executed[0].1["source"],
        json!({"kind": "recipe", "instanceId": instance,
               "recipeName": "Motion Light", "recipeId": "motion-light"})
    );
    for (t, e) in &heard {
        if e["type"] == "equipment.order.executed"
            || e["type"] == "equipment.data.changed" && e["alias"] == "state"
        {
            println!("{t:6.1}s {e}");
        }
    }
    assert_eq!(serving.interrupt(), Some(130));
}

#[test]
fn origins_are_checked_as_sowel_does_and_a_bad_one_is_closed_with_4003() {
    let serving = started();
    let host = format!("127.0.0.1:{}", serving.port);
    let same = format!("http://{host}");
    for origin in [None, Some(same.as_str()), Some("http://localhost:5173")] {
        let extra: Vec<(&str, &str)> = origin.map(|o| ("Origin", o)).into_iter().collect();
        let (_, mut client) = connect_with(serving.port, &extra);
        client.welcome();
    }
    for origin in [
        "http://evil.example",
        "http://localhost:3000",
        "evil",
        "null",
        "http://",
    ] {
        let (_, mut client) = connect_with(serving.port, &[("Origin", origin)]);
        let closed = client.closed_with(Duration::from_secs(5));
        assert_eq!(
            closed,
            Some((4003, "Origin not allowed".to_string())),
            "{origin}"
        );
    }
}

#[test]
fn a_foreign_host_is_a_403_before_the_upgrade() {
    let serving = started();
    let request = upgrade_request(serving.port, "/ws", "GET", &[("Host", "evil.example")]);
    let (answer, _, _) = open(serving.port, &request);
    assert_eq!(answer.status, 403);
    assert!(answer.body.contains("Forbidden host"), "{}", answer.body);
}

#[test]
fn a_fifth_client_is_a_503_busy_and_a_closed_client_gives_its_slot_back() {
    let serving = started();
    let mut clients: Vec<Client> = (0..4).map(|_| connect(serving.port)).collect();
    for client in &mut clients {
        client.welcome();
    }
    let (answer, _, _) = open(
        serving.port,
        &upgrade_request(serving.port, "/ws", "GET", &[]),
    );
    assert_eq!(answer.status, 503);
    assert_eq!(
        serde_json::from_str::<Value>(&answer.body).unwrap(),
        json!({"error": "Busy"})
    );
    // The four HTTP workers are still free.
    let (status, _) = serving.send("GET", "/api/v1/zones", &Value::Null);
    assert_eq!(status, 200);
    // One goes: a new one connects.
    drop(clients.remove(0));
    let start = Instant::now();
    loop {
        let (answer, stream, rest) = open(
            serving.port,
            &upgrade_request(serving.port, "/ws", "GET", &[]),
        );
        if answer.status == 101 {
            drop((stream, rest));
            break;
        }
        assert_eq!(answer.status, 503);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the slot was not freed"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_frame_of_two_kilobytes_closes_the_client_with_1009() {
    let serving = started();
    let mut client = connect(serving.port);
    client.welcome();
    let big = "a".repeat(2048);
    client.ws.send(Message::Text(big.into())).unwrap();
    let closed = client.closed_with(Duration::from_secs(5));
    assert_eq!(closed.map(|c| c.0), Some(1009));
}

#[test]
fn a_client_that_never_reads_is_dropped_within_ten_seconds_while_the_others_and_http_go_on() {
    let serving = started();
    let zone = serving.zone("Garage");
    // The slow client: a tiny receive buffer, a subscribe to the zones, and nothing read.
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, None).unwrap();
    socket.set_recv_buffer_size(1024).unwrap();
    socket
        .connect(&std::net::SocketAddr::from(([127, 0, 0, 1], serving.port)).into())
        .unwrap();
    let mut slow = TcpStream::from(socket);
    slow.write_all(upgrade_request(serving.port, "/ws", "GET", &[]).as_bytes())
        .unwrap();
    slow.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut head = [0u8; 512];
    let n = slow.read(&mut head).unwrap();
    assert!(String::from_utf8_lossy(&head[..n]).starts_with("HTTP/1.1 101"));
    // A masked text frame: the subscribe (a client must mask).
    let message = br#"{"type":"subscribe","topics":["zones"]}"#;
    let mask = [1u8, 2, 3, 4];
    let mut frame = vec![0x81, 0x80 | u8::try_from(message.len()).unwrap()];
    frame.extend_from_slice(&mask);
    frame.extend(message.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    slow.write_all(&frame).unwrap();
    // A client that reads the devices (the simulator reports all the time), and two idle ones:
    // the four slots are taken.
    let mut bystander = connect(serving.port);
    bystander.welcome();
    bystander.subscribe(&["devices"]);
    let idle: Vec<Client> = (0..2)
        .map(|_| {
            let mut client = connect(serving.port);
            client.welcome();
            client
        })
        .collect();
    thread::sleep(Duration::from_millis(400));
    let (answer, _, _) = open(
        serving.port,
        &upgrade_request(serving.port, "/ws", "GET", &[]),
    );
    assert_eq!(answer.status, 503, "four clients are connected");
    // More events in a moment than a client's queue holds (8 KB): the slow client cannot be
    // served; it is closed and its slot comes back. (Kernel buffers swallow whatever a client
    // that does not read is sent at a trickle, so the burst is what shows it.)
    for n in 0..300 {
        let (status, _) = serving.send(
            "PUT",
            &format!("/api/v1/zones/{zone}"),
            &json!({"name": format!("Garage with a rather long name to make events heavier {n}")}),
        );
        assert_eq!(status, 200);
    }
    let start = Instant::now();
    loop {
        let (answer, stream, rest) = open(
            serving.port,
            &upgrade_request(serving.port, "/ws", "GET", &[]),
        );
        if answer.status == 101 {
            drop((stream, rest));
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the slow client was not dropped"
        );
        thread::sleep(Duration::from_millis(100));
    }
    // It was told why: its last bytes are a close frame, 1013.
    let mut rest = Vec::new();
    let _ = slow.read_to_end(&mut rest);
    let mut tail = vec![0x88, 0x11, 0x03, 0xF5];
    tail.extend_from_slice(b"Try again later");
    assert!(
        rest.ends_with(&tail),
        "{:?}",
        &rest[rest.len().saturating_sub(24)..]
    );
    // The others are unaffected: the bystander still gets the devices, HTTP answers.
    assert!(
        bystander
            .event_of("device.data.updated", Duration::from_secs(10))
            .is_some()
    );
    let (status, _) = serving.send("GET", "/api/v1/zones", &Value::Null);
    assert_eq!(status, 200);
    drop(idle);
}

#[test]
fn a_silent_client_is_dropped_and_a_reading_one_is_pinged_and_stays() {
    let dir = data_dir();
    pair(&dir);
    // Pings every second, silence of three seconds (the defaults are 30 and 60).
    let serving = serve(
        &dir,
        "100",
        &[
            ("SOLITE_WS_PING_SECONDS", "1"),
            ("SOLITE_WS_SILENCE_SECONDS", "3"),
        ],
    );
    // Three silent clients: they read nothing, so no pong ever goes back.
    let silent: Vec<_> = (0..3)
        .map(|_| {
            let (answer, stream, _) = open(
                serving.port,
                &upgrade_request(serving.port, "/ws", "GET", &[]),
            );
            assert_eq!(answer.status, 101);
            stream
        })
        .collect();
    // A reader: answers the pings (the library does) and receives them.
    let mut reader = connect(serving.port);
    reader.welcome();
    let (answer, _, _) = open(
        serving.port,
        &upgrade_request(serving.port, "/ws", "GET", &[]),
    );
    assert_eq!(answer.status, 503, "four clients");
    let start = Instant::now();
    let mut pings = 0;
    while start.elapsed() < Duration::from_secs(7) {
        if let Ok(Some(Message::Ping(_))) = reader.next(Duration::from_millis(300)) {
            pings += 1;
        }
    }
    assert!(pings >= 3, "{pings} pings in 7 s");
    // The three silent ones are gone: three new clients fit; the reader is still there (a
    // fifth would be refused).
    let again: Vec<_> = (0..3)
        .map(|_| {
            let (answer, stream, _) = open(
                serving.port,
                &upgrade_request(serving.port, "/ws", "GET", &[]),
            );
            assert_eq!(answer.status, 101, "the silent clients were dropped");
            stream
        })
        .collect();
    let (answer, _, _) = open(
        serving.port,
        &upgrade_request(serving.port, "/ws", "GET", &[]),
    );
    assert_eq!(answer.status, 503, "the reader is still connected");
    drop((silent, again));
}

#[test]
fn a_request_for_ws_without_an_upgrade_a_wrong_version_or_another_method_is_refused() {
    let serving = started();
    let plain = "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:PORT\r\nConnection: close\r\n\r\n"
        .replace("PORT", &serving.port.to_string());
    let (answer, _, _) = open(serving.port, &plain);
    assert_eq!(answer.status, 426);
    assert_eq!(
        serde_json::from_str::<Value>(&answer.body).unwrap(),
        json!({"error": "Upgrade Required"})
    );
    let (answer, _, _) = open(
        serving.port,
        &upgrade_request(
            serving.port,
            "/ws",
            "GET",
            &[("Sec-WebSocket-Version", "8")],
        ),
    );
    assert_eq!(answer.status, 400);
    assert_eq!(
        serde_json::from_str::<Value>(&answer.body).unwrap(),
        json!({"error": "Unsupported WebSocket version"})
    );
    let (answer, _, _) = open(
        serving.port,
        &upgrade_request(serving.port, "/ws", "POST", &[]),
    );
    assert_eq!(answer.status, 405);
    // A key is needed to answer an upgrade.
    let (answer, _, _) = open(
        serving.port,
        &upgrade_request(serving.port, "/ws", "GET", &[("Sec-WebSocket-Key", "")]),
    );
    assert_eq!(answer.status, 426);
}

#[test]
fn a_binary_subscribe_is_applied_and_a_topics_that_is_not_a_list_is_ignored() {
    let serving = started();
    let mut client = connect(serving.port);
    client.welcome();
    // A binary frame, read as text.
    let message = json!({"type": "subscribe", "topics": ["zones"]}).to_string();
    client
        .ws
        .send(Message::Binary(Bytes::from(message.into_bytes())))
        .unwrap();
    thread::sleep(Duration::from_millis(400));
    serving.zone("One");
    assert!(
        client
            .event_of("zone.created", Duration::from_secs(3))
            .is_some()
    );
    // `topics` a string: ignored, the zones stay subscribed. A binary that is not UTF-8: ignored.
    client
        .ws
        .send(Message::Text(
            r#"{"type":"subscribe","topics":"devices"}"#.into(),
        ))
        .unwrap();
    client
        .ws
        .send(Message::Binary(Bytes::from(vec![0xff, 0xfe, 0x00])))
        .unwrap();
    thread::sleep(Duration::from_millis(400));
    serving.zone("Two");
    assert!(
        client
            .event_of("zone.created", Duration::from_secs(3))
            .is_some()
    );
    // `[1, "zones"]`: the entry that is not a string is ignored, zones applied.
    let mut other = connect(serving.port);
    other.welcome();
    other
        .ws
        .send(Message::Text(
            r#"{"type":"subscribe","topics":[1,"zones"]}"#.into(),
        ))
        .unwrap();
    thread::sleep(Duration::from_millis(400));
    serving.zone("Three");
    assert!(
        other
            .event_of("zone.created", Duration::from_secs(3))
            .is_some()
    );
    // Before any subscribe only `system` is listened to: nothing comes.
    let mut quiet = connect(serving.port);
    quiet.welcome();
    serving.zone("Four");
    assert_eq!(quiet.text(Duration::from_millis(800)), None);
}

#[test]
fn a_ping_is_answered_with_a_pong() {
    let serving = started();
    let mut client = connect(serving.port);
    client.welcome();
    client
        .ws
        .send(Message::Ping(Bytes::from_static(b"hi")))
        .unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    let mut pong = false;
    while Instant::now() < end && !pong {
        if let Ok(Some(Message::Pong(data))) = client.next(Duration::from_millis(300)) {
            assert_eq!(&data[..], b"hi");
            pong = true;
        }
    }
    assert!(pong, "no pong");
}

#[test]
fn ctrl_c_gives_each_client_a_close_1001_and_the_host_exits() {
    let serving = started();
    let mut one = connect(serving.port);
    one.welcome();
    let mut two = connect(serving.port);
    two.welcome();
    two.subscribe(&["zones"]);
    let handle = thread::spawn(move || {
        let closed = one.closed_with(Duration::from_secs(10));
        let closed_two = two.closed_with(Duration::from_secs(10));
        (closed, closed_two)
    });
    thread::sleep(Duration::from_millis(300));
    assert_eq!(serving.interrupt(), Some(130));
    let (one, two) = handle.join().unwrap();
    assert_eq!(one.map(|c| c.0), Some(1001));
    assert_eq!(two.map(|c| c.0), Some(1001));
    // The code is the library's `Away`.
    assert_eq!(u16::from(CloseCode::Away), 1001);
}

#[test]
fn a_client_that_sends_close_1000_gets_the_close_frame_back() {
    let serving = started();
    let mut client = connect(serving.port);
    client.welcome();
    client
        .ws
        .close(Some(tungstenite::protocol::CloseFrame {
            code: CloseCode::Normal,
            reason: "bye".into(),
        }))
        .unwrap();
    let closed = client.closed_with(Duration::from_secs(5));
    assert_eq!(closed.map(|c| c.0), Some(1000));
}

/// Writes a masked frame (mask 0) of this first byte and payload on the client's socket.
fn raw_frame(client: &mut Client, first: u8, payload: &[u8]) {
    let mut frame = vec![first, 0x80 | payload.len() as u8, 0, 0, 0, 0];
    frame.extend_from_slice(payload);
    let stream = client.ws.get_mut();
    stream.write_all(&frame).unwrap();
    stream.flush().unwrap();
}

#[test]
fn a_protocol_error_is_closed_with_1002_and_a_text_frame_that_is_not_utf8_with_1007() {
    let serving = started();
    let mut client = connect(serving.port);
    client.welcome();
    // A final frame with a reserved opcode.
    raw_frame(&mut client, 0x83, b"x");
    assert_eq!(
        client.closed_with(Duration::from_secs(5)).map(|c| c.0),
        Some(1002)
    );
    let mut client = connect(serving.port);
    client.welcome();
    raw_frame(&mut client, 0x81, &[0xff, 0xfe, 0xfd]);
    assert_eq!(
        client.closed_with(Duration::from_secs(5)).map(|c| c.0),
        Some(1007)
    );
}

/// Sends these bytes on a new connection and reads until the host ends it (or 5 s).
fn raw_exchange(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    String::from_utf8_lossy(&answer).to_string()
}

#[test]
fn an_upgrade_anywhere_but_on_ws_is_a_400_and_never_pins_an_api_worker() {
    let serving = started();
    let port = serving.port;
    let upgrade_on = |target: &str| {
        format!(
            "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: {KEY}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
    };
    for target in [
        "/ws/".to_string(),
        format!("http://127.0.0.1:{port}/ws"),
        "/api/v1/zones".to_string(),
    ] {
        let answer = raw_exchange(port, &upgrade_on(&target));
        assert!(answer.starts_with("HTTP/1.1 400"), "{target}: {answer}");
        assert!(
            answer.contains(r#"{"error":"Upgrade only on /ws"}"#),
            "{target}: {answer}"
        );
    }
    // A second request on a kept-alive connection: the connection ends after the first answer.
    let kept = format!(
        "GET /api/v1/zones HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: keep-alive\r\n\r\n{}",
        upgrade_on("/api/v1/zones")
    );
    let answer = raw_exchange(port, &kept);
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(!answer.contains("101 Switching"), "{answer}");
    // Many of them, held open: the API still answers (no worker is pinned).
    let held: Vec<TcpStream> = (0..6)
        .map(|_| {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.write_all(upgrade_on("/ws/").as_bytes()).unwrap();
            stream
        })
        .collect();
    for _ in 0..3 {
        let (status, _) = serving.send("GET", "/api/v1/zones", &Value::Null);
        assert_eq!(status, 200);
    }
    drop(held);
}

#[test]
fn a_client_on_devices_gets_one_device_discovered_per_new_device_when_pairing_opens() {
    let dir = data_dir();
    let mut serving = serve(&dir, "10", &[]);
    serving.dir = Some(dir);
    let mut client = connect(serving.port);
    client.welcome();
    client.subscribe(&["devices"]);
    thread::sleep(Duration::from_millis(400));
    let (status, _) = serving.send(
        "POST",
        "/api/v1/integrations/solite/permit-join",
        &json!({"time": 30}),
    );
    assert_eq!(status, 202);
    let mut found: Vec<Value> = Vec::new();
    let end = Instant::now() + Duration::from_secs(25);
    while found.len() < 3 && Instant::now() < end {
        found.extend(
            client
                .events(Duration::from_secs(1))
                .into_iter()
                .filter(|e| e["type"] == "device.discovered"),
        );
    }
    assert_eq!(found.len(), 3, "{found:?}");
    let mut models: Vec<&str> = found
        .iter()
        .map(|e| e["device"]["model"].as_str().unwrap())
        .collect();
    models.sort_unstable();
    assert_eq!(models, ["MS01", "TH01", "ZBMINIR2"]);
    for event in &found {
        assert!(event["device"].get("data").is_none());
        assert_eq!(event["device"]["integrationId"], "solite");
    }
}
