//! A release found and pulled by the box (spec 039): the manifest and its rules, where the box
//! may fetch from, the check and pull routes behind the gate, the pull's bounds and failures, the
//! automatic check, the settings. A fake `Fetcher` serves URLs from a map; a fake `Pacing` has a
//! clock the test moves; key pairs are made here from random bytes.

mod support;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{self, Cursor, ErrorKind, Read};
use std::rc::Rc;
use std::sync::Mutex;

use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use solite_core::api::{Reach, Reply, Request, handle, handle_guarded};
use solite_core::auth::{Account, Language, PasswordHash};
use solite_core::backup::Pacing;
use solite_core::devices::Id;
use solite_core::home::Home;
use solite_core::supervisor::Mode;
use solite_core::update::release::{
    DEFAULT_MANIFEST_URL, Fetched, Fetcher, Limits, MAX_MANIFEST_BYTES,
};
use solite_core::update::{HEADER_BYTES, SLOT_BYTES, Updates, Version, signed_bytes};
use support::update_fake::FakeAdapter;
use support::{HOST, PORT, T0, counter};

const ASSET: &str = "https://github.com/o/r/releases/download/v0.2.0/solite-0.2.0.sup";
const STORE: &str = "https://objects.example.com/signed/solite-0.2.0.sup";
const SECOND: u64 = 1000;

// ---- Keys and files ----

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .expect("random bytes");
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn running() -> Version {
    Version::parse(solite_core::VERSION).unwrap()
}

fn newer() -> Version {
    Version::new(0, 2, 0)
}

fn image(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from((i * 7 + i / 251 + 3) % 256).unwrap())
        .collect()
}

fn header(signing: &SigningKey, version: Version, length: u32, digest: &[u8; 32]) -> Vec<u8> {
    let signed = signed_bytes(version, length, digest);
    let mut out = signed.to_vec();
    out.extend_from_slice(&signing.sign(&signed).to_bytes());
    out
}

/// The signed update file: the header, then the image.
fn file(signing: &SigningKey, version: Version, image: &[u8]) -> Vec<u8> {
    let mut out = header(
        signing,
        version,
        u32::try_from(image.len()).unwrap(),
        &sha256(image),
    );
    out.extend_from_slice(image);
    out
}

/// A manifest of spec 038 R2 for `sup`, with the fields a test may bend.
fn manifest_for(sup: &[u8], version: &str, url: &str) -> Value {
    json!({
        "format": 1,
        "version": version,
        "minVersion": null,
        "releasedAt": "2026-11-26T10:00:00Z",
        "notes": "Some notes.",
        "image": {
            "name": format!("solite-{version}.sup"),
            "size": sup.len(),
            "sha256": hex(&sha256(sup)),
            "url": url,
        },
    })
}

// ---- The fakes ----

#[derive(Clone)]
enum End {
    Eof,
    Error(ErrorKind),
}

#[derive(Clone)]
enum Answer {
    Body {
        data: Vec<u8>,
        length: Option<u64>,
        end: End,
        /// Milliseconds the clock moves at each read.
        tick: u64,
    },
    Redirect(Option<String>),
    Status(u16),
    Fail(String),
}

fn body(data: &[u8]) -> Answer {
    Answer::Body {
        data: data.to_vec(),
        length: Some(data.len() as u64),
        end: End::Eof,
        tick: 0,
    }
}

fn body_no_length(data: &[u8]) -> Answer {
    Answer::Body {
        data: data.to_vec(),
        length: None,
        end: End::Eof,
        tick: 0,
    }
}

fn redirect(to: &str) -> Answer {
    Answer::Redirect(Some(to.to_string()))
}

struct Script {
    data: Cursor<Vec<u8>>,
    end: End,
    tick: u64,
    clock: Rc<Cell<u64>>,
}

impl Read for Script {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.clock.set(self.clock.get() + self.tick);
        // Small reads, so that a cut or a delay happens between reads.
        let max = out.len().min(1000);
        let count = self.data.read(&mut out[..max])?;
        if count == 0
            && let End::Error(kind) = self.end
        {
            return Err(io::Error::new(kind, "scripted"));
        }
        Ok(count)
    }
}

struct FakeFetcher {
    answers: HashMap<String, Answer>,
    log: Rc<RefCell<Vec<String>>>,
    clock: Rc<Cell<u64>>,
}

impl Fetcher for FakeFetcher {
    fn get(&mut self, url: &str, _idle_ms: u64) -> Result<Fetched, String> {
        self.log.borrow_mut().push(url.to_string());
        match self.answers.get(url).cloned() {
            None => Ok(Fetched::Status(404)),
            Some(Answer::Fail(message)) => Err(message),
            Some(Answer::Status(status)) => Ok(Fetched::Status(status)),
            Some(Answer::Redirect(location)) => Ok(Fetched::Redirect(location)),
            Some(Answer::Body {
                data,
                length,
                end,
                tick,
            }) => Ok(Fetched::Body {
                length,
                body: Box::new(Script {
                    data: Cursor::new(data),
                    end,
                    tick,
                    clock: Rc::clone(&self.clock),
                }),
            }),
        }
    }
}

struct FakePacing {
    clock: Rc<Cell<u64>>,
    pauses: Rc<Cell<u32>>,
}

impl Pacing for FakePacing {
    fn now_ms(&mut self) -> u64 {
        self.clock.get()
    }

    fn pause(&mut self) {
        self.pauses.set(self.pauses.get() + 1);
    }
}

/// The limits the tests use: short ones.
const CHECK: Limits = Limits {
    idle_ms: 100,
    total_ms: 5 * SECOND,
};
const DOWNLOAD: Limits = Limits {
    idle_ms: 100,
    total_ms: 10 * SECOND,
};

// ---- The world of a test ----

struct World {
    home: Mutex<Home>,
    random: Mutex<support::Random>,
    token: String,
    now: Cell<u64>,
    signing: SigningKey,
    updates: Updates,
    adapter: FakeAdapter,
    fetcher: FakeFetcher,
    clock: Rc<Cell<u64>>,
    pauses: Rc<Cell<u32>>,
}

impl World {
    fn new() -> World {
        let signing = SigningKey::from_bytes(&random_bytes::<32>());
        let (updates, line) =
            Updates::from_key_text(Some(&hex(&signing.verifying_key().to_bytes())));
        assert!(updates.has_key(), "{line}");
        World::with(signing, updates, T0)
    }

    fn without_key() -> World {
        let signing = SigningKey::from_bytes(&random_bytes::<32>());
        let (updates, _) = Updates::from_key_text(None);
        World::with(signing, updates, T0)
    }

    /// A world whose wall clock is before 2020 (not set yet) from the login on.
    fn with_clock_not_set() -> World {
        let signing = SigningKey::from_bytes(&random_bytes::<32>());
        let (updates, _) = Updates::from_key_text(Some(&hex(&signing.verifying_key().to_bytes())));
        World::with(signing, updates, 1_500_000_000_000)
    }

    fn with(signing: SigningKey, updates: Updates, now: u64) -> World {
        let mut home = Home::new(T0);
        home.load_auth(Some(Account {
            id: Id::parse("11111111-1111-4111-8111-111111111111").unwrap(),
            username: "admin".into(),
            display_name: "Owner".into(),
            language: Language::En,
            created_at: T0,
            password: PasswordHash::new("correct horse battery", [7; 16]),
        }));
        let clock = Rc::new(Cell::new(1_000_000));
        let mut world = World {
            home: Mutex::new(home),
            random: Mutex::new(counter()),
            token: String::new(),
            now: Cell::new(now),
            signing,
            updates,
            adapter: FakeAdapter::new(),
            fetcher: FakeFetcher {
                answers: HashMap::new(),
                log: Rc::new(RefCell::new(Vec::new())),
                clock: Rc::clone(&clock),
            },
            clock,
            pauses: Rc::new(Cell::new(0)),
        };
        let login = br#"{"username":"admin","password":"correct horse battery"}"#;
        let reply = world.send(
            "POST",
            "/api/v1/auth/login",
            None,
            Some("application/json"),
            login,
            None,
        );
        let (status, answer) = whole(reply);
        assert_eq!(status, 200, "{answer}");
        world.token = answer["accessToken"].as_str().unwrap().to_string();
        world
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        content_type: Option<&str>,
        body: &[u8],
        content_length: Option<u64>,
    ) -> Reply {
        let authorization = token.map(|t| format!("Bearer {t}"));
        let mut random = |buffer: &mut [u8]| (self.random.lock().unwrap())(buffer);
        handle_guarded(
            &self.home,
            &Request {
                method,
                path,
                host: Some(HOST),
                port: PORT,
                reach: Reach::Loopback,
                content_type,
                authorization: authorization.as_deref(),
                content_length,
                body,
            },
            self.now.get(),
            &mut random,
        )
    }

    fn request(&self, method: &str, path: &str, content_length: Option<u64>) -> Reply {
        let token = self.token.clone();
        self.send(method, path, Some(&token), None, b"", content_length)
    }

    /// Runs the reply the core gave: a check, a pull, or a whole answer.
    fn run(&mut self, reply: Reply) -> (u16, Value) {
        let mut pacing = FakePacing {
            clock: Rc::clone(&self.clock),
            pauses: Rc::clone(&self.pauses),
        };
        let response = match reply {
            Reply::VersionCheck(ticket) => ticket.check(&mut self.fetcher, &mut pacing, CHECK),
            Reply::UpdatePull(ticket) => ticket.pull(
                &self.updates,
                &mut self.fetcher,
                &mut self.adapter,
                &mut pacing,
                CHECK,
                DOWNLOAD,
            ),
            Reply::Whole(response) => response,
            other => panic!("unexpected reply {other:?}"),
        };
        (
            response.status,
            response
                .body
                .map_or(Value::Null, |t| serde_json::from_str(&t).unwrap()),
        )
    }

    fn check(&mut self) -> (u16, Value) {
        let reply = self.request("POST", CHECK_PATH, Some(0));
        self.run(reply)
    }

    fn pull(&mut self) -> (u16, Value) {
        let reply = self.request("POST", UPDATE_PATH, Some(0));
        self.run(reply)
    }

    fn version(&mut self) -> (u16, Value) {
        let reply = self.request("GET", VERSION_PATH, None);
        self.run(reply)
    }

    fn elapse(&self, ms: u64) {
        self.clock.set(self.clock.get() + ms);
    }

    fn serve(&mut self, url: &str, answer: Answer) {
        self.fetcher.answers.insert(url.to_string(), answer);
    }

    fn fetched(&self) -> Vec<String> {
        self.fetcher.log.borrow().clone()
    }

    /// Serves a manifest at the default address.
    fn serve_manifest(&mut self, manifest: &Value) {
        self.serve(DEFAULT_MANIFEST_URL, body(manifest.to_string().as_bytes()));
    }

    /// A release of `version` with an image of `len` bytes: its manifest, and the file behind one
    /// redirect (GitHub's asset address, then the store). Returns the file.
    fn serve_release(&mut self, version: Version, len: usize) -> Vec<u8> {
        let sup = file(&self.signing, version, &image(len));
        self.serve_release_file(&version.to_string(), &sup);
        sup
    }

    fn serve_release_file(&mut self, version: &str, sup: &[u8]) {
        self.serve_manifest(&manifest_for(sup, version, ASSET));
        self.serve(ASSET, redirect(STORE));
        self.serve(STORE, body(sup));
    }

    fn set_setting(&self, key: &str, value: &str) {
        let mut entries = Map::new();
        entries.insert(key.to_string(), json!(value));
        self.home.lock().unwrap().put_settings(&entries).unwrap();
    }

    /// Nothing bootable was left.
    fn untouched(&self) {
        assert!(!self.adapter.next_boot_is_inactive);
        assert!(!self.adapter.pending);
        assert!(self.adapter.slot.is_empty());
    }
}

const CHECK_PATH: &str = "/api/v1/system/version/check";
const UPDATE_PATH: &str = "/api/v1/system/update";
const VERSION_PATH: &str = "/api/v1/system/version";

fn whole(reply: Reply) -> (u16, Value) {
    match reply {
        Reply::Whole(response) => (
            response.status,
            response
                .body
                .map_or(Value::Null, |t| serde_json::from_str(&t).unwrap()),
        ),
        other => panic!("not a whole answer: {other:?}"),
    }
}

fn is_error(answer: &(u16, Value), status: u16, message: &str) {
    assert_eq!(answer.0, status, "{}", answer.1);
    assert_eq!(answer.1, json!({ "error": message }));
}

// ---- The manifest ----

#[test]
fn a_nominal_manifest_is_read_and_kept() {
    let mut world = World::new();
    let sup = world.serve_release(newer(), 3000);
    let (status, info) = world.check();
    assert_eq!(status, 200, "{info}");
    assert_eq!(info["latest"], "0.2.0");
    assert_eq!(info["updateAvailable"], true);
    // The kept fields are the ones a pull then uses: size and hash of the file, no re-fetch.
    let (status, done) = world.pull();
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["message"], "Installed 0.2.0, restarting");
    assert_eq!(world.adapter.slot.len(), sup.len() - HEADER_BYTES);
}

#[test]
fn a_manifest_of_another_format_is_refused() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    manifest["format"] = json!(2);
    world.serve_manifest(&manifest);
    is_error(
        &world.check(),
        500,
        "Update check failed: Unsupported manifest format 2",
    );
}

#[test]
fn a_manifest_that_breaks_a_rule_names_the_field() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let good = manifest_for(&sup, "0.2.0", ASSET);
    let slot_max = HEADER_BYTES as u64 + SLOT_BYTES;
    let cases: Vec<(&str, Value)> = vec![
        ("version", {
            let mut m = good.clone();
            m["version"] = json!("1.2");
            m
        }),
        ("minVersion", {
            let mut m = good.clone();
            m["minVersion"] = json!("x");
            m
        }),
        ("image.size", {
            let mut m = good.clone();
            m["image"]["size"] = json!(0);
            m
        }),
        ("image.size", {
            let mut m = good.clone();
            m["image"]["size"] = json!(HEADER_BYTES);
            m
        }),
        ("image.size", {
            let mut m = good.clone();
            m["image"]["size"] = json!(slot_max + 1);
            m
        }),
        ("image.sha256", {
            let mut m = good.clone();
            m["image"]["sha256"] = json!("a".repeat(63));
            m
        }),
        ("image.url", {
            let mut m = good.clone();
            m["image"]["url"] = json!("http://example.com/solite.sup");
            m
        }),
    ];
    for (field, manifest) in cases {
        world.serve_manifest(&manifest);
        world.elapse(2 * MINUTE);
        is_error(
            &world.check(),
            500,
            &format!("Update check failed: Invalid manifest: {field}"),
        );
    }
    // Not JSON at all.
    world.serve(DEFAULT_MANIFEST_URL, body(b"<html>not json"));
    world.elapse(2 * MINUTE);
    assert_eq!(world.check().0, 500);
    // The largest size is accepted.
    let mut max = good;
    max["image"]["size"] = json!(slot_max);
    world.serve_manifest(&max);
    world.elapse(2 * MINUTE);
    assert_eq!(world.check().0, 200);
}

const MINUTE: u64 = 60 * SECOND;

#[test]
fn unknown_keys_and_long_notes_are_accepted_and_not_kept() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    manifest["notes"] = json!("n".repeat(8000));
    manifest["future"] = json!({"a": [1, 2, 3]});
    world.serve_manifest(&manifest);
    let (status, info) = world.check();
    assert_eq!(status, 200, "{info}");
    assert!(info.get("notes").is_none() && info.get("future").is_none());
}

#[test]
fn a_manifest_of_16_kb_plus_one_is_too_large() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    // Exactly the limit passes, one more byte does not.
    let base = manifest.to_string().len();
    manifest["notes"] = json!("n".repeat(MAX_MANIFEST_BYTES - base + "\"notes\":\"\",".len() - 1));
    let text = manifest.to_string();
    assert!(text.len() <= MAX_MANIFEST_BYTES, "{}", text.len());
    world.serve(DEFAULT_MANIFEST_URL, body(text.as_bytes()));
    assert_eq!(world.check().0, 200);
    let mut over = text.into_bytes();
    over.resize(MAX_MANIFEST_BYTES + 1, b' ');
    world.serve(DEFAULT_MANIFEST_URL, body_no_length(&over));
    world.elapse(2 * MINUTE);
    is_error(
        &world.check(),
        500,
        "Update check failed: Manifest too large",
    );
}

#[test]
fn the_release_page_is_derived_from_a_github_asset_address_only() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    let (_, info) = world.check();
    assert_eq!(
        info["releaseUrl"],
        "https://github.com/o/r/releases/tag/v0.2.0"
    );
    world.serve_manifest(&manifest_for(
        &sup,
        "0.2.0",
        "https://example.com/a/solite.sup",
    ));
    world.elapse(2 * MINUTE);
    let (_, info) = world.check();
    assert_eq!(info["releaseUrl"], Value::Null);
    assert_eq!(world.version().1["releaseUrl"], Value::Null);
}

// ---- The URL rule ----

fn manifest_url_check(world: &mut World, url: &str) -> (u16, Value) {
    let sup = file(&world.signing, newer(), &image(2000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", url));
    world.elapse(2 * MINUTE);
    world.check()
}

#[test]
fn https_to_any_host_and_http_to_the_local_network_are_accepted() {
    let mut world = World::new();
    for url in [
        "https://example.com/a.sup",
        "https://EXAMPLE.com:8443/a.sup?x=1#frag",
        "http://192.168.1.20:8000/a.sup",
        "http://10.0.0.5/a.sup",
        "http://172.16.0.1/a.sup",
        "http://172.31.255.255/a.sup",
        "http://127.0.0.1:9/a.sup",
        "http://localhost:8000/a.sup",
        "HTTP://LocalHost/a.sup",
        "https://host.local",
    ] {
        let answer = manifest_url_check(&mut world, url);
        assert_eq!(answer.0, 200, "{url}: {}", answer.1);
    }
}

#[test]
fn plain_http_elsewhere_and_other_schemes_are_refused() {
    let mut world = World::new();
    for url in [
        "http://8.8.8.8/a.sup",
        "http://example.com/a.sup",
        "http://172.32.0.1/a.sup",
        "http://127.1/a.sup",
        "http://0x7f.0.0.1/a.sup",
        "http://010.0.0.1/a.sup",
        "http://256.1.1.1/a.sup",
        "ftp://192.168.1.1/a.sup",
    ] {
        let answer = manifest_url_check(&mut world, url);
        is_error(
            &answer,
            500,
            "Update check failed: Invalid manifest: image.url",
        );
    }
}

#[test]
fn a_url_of_the_wrong_form_is_invalid() {
    let mut world = World::new();
    let long = format!("https://example.com/{}", "a".repeat(2049 - 20));
    let just = format!("https://example.com/{}", "a".repeat(2048 - 20));
    let bad = [
        "https://example.com/a b".to_string(),
        "https://example.com/a\r\nb".to_string(),
        "https://example.com/é".to_string(),
        "https://u@example.com/a".to_string(),
        "https://[::1]/a".to_string(),
        "https://example.com:0/a".to_string(),
        "https://example.com:70000/a".to_string(),
        "https://example.com:/a".to_string(),
        "https://example.com?x=1".to_string(),
        "https:///a".to_string(),
        long,
    ];
    for url in &bad {
        let answer = manifest_url_check(&mut world, url);
        is_error(
            &answer,
            500,
            "Update check failed: Invalid manifest: image.url",
        );
    }
    assert_eq!(manifest_url_check(&mut world, &just).0, 200);
    // A setting of the wrong form is refused as well (R7).
    let mut entries = Map::new();
    entries.insert("update.manifestUrl".into(), json!("https://u@h/a"));
    let refused = world
        .home
        .lock()
        .unwrap()
        .put_settings(&entries)
        .unwrap_err();
    assert_eq!(refused.message, "Invalid value for update.manifestUrl");
}

// ---- Redirects ----

#[test]
fn one_and_two_redirects_are_followed_absolute_or_by_path() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let manifest = manifest_for(&sup, "0.2.0", ASSET).to_string();
    world.serve(
        DEFAULT_MANIFEST_URL,
        redirect("https://github.com/mchacher/solite/releases/download/v0.2.0/manifest.json"),
    );
    world.serve(
        "https://github.com/mchacher/solite/releases/download/v0.2.0/manifest.json",
        redirect("/final/manifest.json"),
    );
    world.serve(
        "https://github.com/final/manifest.json",
        body(manifest.as_bytes()),
    );
    assert_eq!(world.check().0, 200);
    assert_eq!(world.fetched().len(), 3);
}

#[test]
fn six_redirects_and_a_loop_are_too_many() {
    let mut world = World::new();
    let hops: Vec<String> = (0..7)
        .map(|i| format!("https://h{i}.example.com/m"))
        .collect();
    world.serve(DEFAULT_MANIFEST_URL, redirect(&hops[0]));
    for i in 0..6 {
        world.serve(&hops[i], redirect(&hops[i + 1]));
    }
    world.serve(&hops[6], body(b"{}"));
    is_error(
        &world.check(),
        500,
        "Update check failed: too many redirects",
    );
    // Five are fine (the manifest is then refused for its content, not for the hops).
    world.serve(&hops[4], body(b"{}"));
    world.elapse(2 * MINUTE);
    let answer = world.check();
    assert_eq!(
        answer.1["error"],
        "Update check failed: Invalid manifest: format"
    );
    // A loop.
    world.serve(DEFAULT_MANIFEST_URL, redirect(DEFAULT_MANIFEST_URL));
    world.elapse(2 * MINUTE);
    is_error(
        &world.check(),
        500,
        "Update check failed: too many redirects",
    );
}

#[test]
fn a_redirect_without_location_or_with_a_bad_one_is_refused() {
    let mut world = World::new();
    world.serve(DEFAULT_MANIFEST_URL, Answer::Redirect(None));
    is_error(
        &world.check(),
        500,
        "Update check failed: redirect without Location",
    );
    for location in ["//h.example.com/x", "x", "ftp://h/x", "http://8.8.8.8/x"] {
        world.serve(DEFAULT_MANIFEST_URL, redirect(location));
        world.elapse(2 * MINUTE);
        let answer = world.check();
        assert_eq!(answer.0, 500, "{location}");
        assert!(
            answer.1["error"] == "Update check failed: invalid URL"
                || answer.1["error"] == "Update check failed: redirect from https to http refused",
            "{location}: {}",
            answer.1
        );
    }
}

#[test]
fn a_redirect_from_https_to_http_is_refused_even_on_the_local_network() {
    let mut world = World::new();
    world.serve(
        DEFAULT_MANIFEST_URL,
        redirect("http://192.168.1.5/manifest.json"),
    );
    is_error(
        &world.check(),
        500,
        "Update check failed: redirect from https to http refused",
    );
}

#[test]
fn a_status_other_than_200_names_the_url_and_keeps_the_cache() {
    let mut world = World::new();
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
    world.elapse(2 * MINUTE);
    world.serve(DEFAULT_MANIFEST_URL, Answer::Status(404));
    is_error(
        &world.check(),
        500,
        &format!("Update check failed: {DEFAULT_MANIFEST_URL}: 404"),
    );
    assert_eq!(world.version().1["latest"], "0.2.0");
    world.elapse(2 * MINUTE);
    world.serve(
        DEFAULT_MANIFEST_URL,
        Answer::Fail("connection refused".into()),
    );
    is_error(
        &world.check(),
        500,
        "Update check failed: connection refused",
    );
}

// ---- The check route ----

#[test]
fn a_newer_manifest_is_available_and_get_version_says_the_same() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    let (status, info) = world.check();
    assert_eq!(status, 200, "{info}");
    assert_eq!(
        info,
        json!({
            "current": solite_core::VERSION,
            "latest": "0.2.0",
            "updateAvailable": true,
            "releaseUrl": "https://github.com/o/r/releases/tag/v0.2.0",
        })
    );
    assert_eq!(world.version(), (200, info));
}

#[test]
fn an_equal_or_older_manifest_is_not_available() {
    let mut world = World::new();
    for version in [running().to_string(), "0.0.1".to_string()] {
        let sup = file(
            &world.signing,
            Version::parse(&version).unwrap(),
            &image(2000),
        );
        world.serve_manifest(&manifest_for(&sup, &version, ASSET));
        world.elapse(2 * MINUTE);
        let (status, info) = world.check();
        assert_eq!(status, 200, "{info}");
        assert_eq!(info["updateAvailable"], false);
        assert_eq!(info["latest"], json!(version));
    }
}

#[test]
fn a_min_version_above_the_running_one_is_not_available() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    manifest["minVersion"] = json!("0.1.9");
    world.serve_manifest(&manifest);
    let (_, info) = world.check();
    assert_eq!(info["updateAvailable"], false);
    assert_eq!(info["latest"], "0.2.0");
    // Equal to the running version: available.
    manifest["minVersion"] = json!(running().to_string());
    world.serve_manifest(&manifest);
    world.elapse(2 * MINUTE);
    assert_eq!(world.check().1["updateAvailable"], true);
}

#[test]
fn before_any_check_the_version_has_no_latest() {
    let mut world = World::new();
    let (status, info) = world.version();
    assert_eq!(status, 200);
    assert_eq!(
        info,
        json!({"current": solite_core::VERSION, "latest": null, "updateAvailable": false, "releaseUrl": null})
    );
    assert!(world.fetched().is_empty());
}

#[test]
fn a_second_check_within_a_minute_is_429_then_accepted() {
    let mut world = World::new();
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
    world.elapse(10 * SECOND);
    is_error(&world.check(), 429, "Please wait 50s before checking again");
    assert_eq!(world.fetched().len(), 1);
    world.elapse(50 * SECOND);
    assert_eq!(world.check().0, 200);
}

#[test]
fn a_check_while_another_job_holds_the_ticket_is_409() {
    let world = World::new();
    let first = world.request("POST", CHECK_PATH, Some(0));
    assert!(matches!(first, Reply::VersionCheck(_)));
    is_error(
        &whole(world.request("POST", CHECK_PATH, Some(0))),
        409,
        "Update check already in progress",
    );
    // A pull meanwhile: the pull's text.
    is_error(
        &whole(world.request("POST", UPDATE_PATH, Some(0))),
        409,
        "Update already in progress",
    );
    drop(first);
    // The ticket's drop ends the job.
    let again = world.request("POST", CHECK_PATH, Some(0));
    assert!(matches!(again, Reply::VersionCheck(_)));
}

#[test]
fn a_check_with_the_clock_not_set_is_503_and_fetches_nothing() {
    let world = World::with_clock_not_set();
    let answer = whole(world.request("POST", CHECK_PATH, Some(0)));
    is_error(
        &answer,
        503,
        "The clock is not set yet: checking a release needs the date",
    );
    let answer = whole(world.request("POST", UPDATE_PATH, Some(0)));
    is_error(
        &answer,
        503,
        "The clock is not set yet: checking a release needs the date",
    );
    assert!(world.fetched().is_empty());
}

#[test]
fn in_safe_mode_the_check_and_the_pull_are_let_through() {
    let mut world = World::new();
    world
        .home
        .lock()
        .unwrap()
        .set_supervision(solite_core::supervisor::Status {
            mode: Mode::Safe,
            boot_cause: "panic",
            last_incident: Some("panic"),
            restarts_last_hour: 3,
            restarts_last_day: 3,
        });
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
    assert_eq!(world.pull().0, 200);
    // Another POST is still refused.
    let (status, _) = whole(world.request("POST", "/api/v1/zones", Some(2)));
    assert_eq!(status, 503);
}

#[test]
fn the_plain_handler_answers_501_to_the_check() {
    let mut home = Home::new(T0);
    let mut random = counter();
    let response = handle(
        &mut home,
        &Request {
            method: "POST",
            path: CHECK_PATH,
            host: Some(HOST),
            port: PORT,
            reach: Reach::Loopback,
            content_type: None,
            authorization: None,
            content_length: Some(0),
            body: b"",
        },
        T0,
        &mut random,
    );
    assert_eq!(response.status, 501);
    assert_eq!(
        response.body.unwrap(),
        r#"{"error":"Updates need the streaming handler"}"#
    );
}

#[test]
fn the_check_needs_no_content_type_and_ignores_a_body() {
    let world = World::new();
    let token = world.token.clone();
    for (content_type, body) in [
        (None, &b""[..]),
        (Some("text/plain"), b"hello"),
        (Some("application/json"), br#"{"a":1}"#),
    ] {
        let reply = world.send(
            "POST",
            CHECK_PATH,
            Some(&token),
            content_type,
            body,
            Some(body.len() as u64),
        );
        assert!(matches!(reply, Reply::VersionCheck(_)), "{reply:?}");
    }
}

#[test]
fn other_methods_on_the_check_route_are_405() {
    let world = World::new();
    for method in ["GET", "PUT", "DELETE"] {
        let (status, _) = whole(world.request(method, CHECK_PATH, Some(0)));
        assert_eq!(status, 405, "{method}");
    }
}

#[test]
fn the_check_route_needs_the_token() {
    let world = World::new();
    let (status, _) = whole(world.send("POST", CHECK_PATH, None, None, b"", Some(0)));
    assert_eq!(status, 401);
    let (status, _) = whole(world.send("POST", UPDATE_PATH, None, None, b"", Some(0)));
    assert_eq!(status, 401);
}

// ---- The update route's two forms ----

#[test]
fn an_empty_body_is_the_pull_whatever_its_content_type() {
    let world = World::new();
    let token = world.token.clone();
    for content_type in [
        None,
        Some("application/json"),
        Some("application/octet-stream"),
    ] {
        let reply = world.send(
            "POST",
            UPDATE_PATH,
            Some(&token),
            content_type,
            b"",
            Some(0),
        );
        assert!(
            matches!(reply, Reply::UpdatePull(_)),
            "{content_type:?}: {reply:?}"
        );
    }
}

#[test]
fn a_body_is_the_upload_and_a_chunked_one_too() {
    let world = World::new();
    let token = world.token.clone();
    let reply = world.send(
        "POST",
        UPDATE_PATH,
        Some(&token),
        Some("application/octet-stream"),
        b"x",
        Some(1),
    );
    assert!(matches!(reply, Reply::Update), "{reply:?}");
    let reply = world.send(
        "POST",
        UPDATE_PATH,
        Some(&token),
        Some("application/octet-stream"),
        b"",
        None,
    );
    assert!(matches!(reply, Reply::Update), "{reply:?}");
    let reply = world.send(
        "POST",
        UPDATE_PATH,
        Some(&token),
        Some("application/json"),
        b"x",
        Some(1),
    );
    assert_eq!(
        whole(reply),
        (415, json!({"error": "Unsupported Media Type"}))
    );
}

// ---- The pull ----

#[test]
fn a_pull_installs_the_release_through_a_redirect() {
    let mut world = World::new();
    let sup = world.serve_release(newer(), 9000);
    let (status, done) = world.pull();
    assert_eq!(status, 200, "{done}");
    assert_eq!(
        done,
        json!({"success": true, "message": "Installed 0.2.0, restarting"})
    );
    assert_eq!(world.adapter.count("finish"), 1);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
    assert_eq!(world.fetched(), vec![DEFAULT_MANIFEST_URL, ASSET, STORE]);
}

#[test]
fn a_fresh_cache_is_used_and_an_old_one_is_fetched_again() {
    let mut world = World::new();
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
    // Younger than ten minutes: the manifest is not fetched again.
    world.elapse(9 * MINUTE);
    assert_eq!(world.pull().0, 200);
    assert_eq!(
        world
            .fetched()
            .iter()
            .filter(|u| *u == DEFAULT_MANIFEST_URL)
            .count(),
        1
    );
    // Older: fetched again (a new world, the first one is on probation).
    let mut world = World::new();
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
    world.elapse(11 * MINUTE);
    assert_eq!(world.pull().0, 200);
    assert_eq!(
        world
            .fetched()
            .iter()
            .filter(|u| *u == DEFAULT_MANIFEST_URL)
            .count(),
        2
    );
}

#[test]
fn a_version_not_newer_is_400_no_update_available() {
    let mut world = World::new();
    let sup = file(&world.signing, running(), &image(2000));
    world.serve_release_file(&running().to_string(), &sup);
    is_error(&world.pull(), 400, "No update available");
    world.untouched();
    assert_eq!(world.adapter.count("begin"), 0);
    assert!(!world.fetched().contains(&ASSET.to_string()));
}

#[test]
fn a_min_version_too_high_is_400() {
    let mut world = World::new();
    let sup = file(&world.signing, Version::new(0, 9, 0), &image(2000));
    let mut manifest = manifest_for(&sup, "0.9.0", ASSET);
    manifest["minVersion"] = json!("0.5.0");
    world.serve_manifest(&manifest);
    is_error(
        &world.pull(),
        400,
        &format!(
            "Version 0.9.0 needs 0.5.0 or later first (running {}): install 0.5.0 before it",
            solite_core::VERSION
        ),
    );
    assert_eq!(world.fetched().len(), 1);
}

#[test]
fn a_min_version_equal_to_the_running_one_is_allowed() {
    let mut world = World::new();
    let sup = world.serve_release(newer(), 2000);
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    manifest["minVersion"] = json!(running().to_string());
    world.serve_manifest(&manifest);
    assert_eq!(world.pull().0, 200);
}

#[test]
fn an_image_above_the_slot_is_413() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    manifest["image"]["size"] = json!(HEADER_BYTES as u64 + SLOT_BYTES);
    world.serve_manifest(&manifest);
    world.adapter.capacity = SLOT_BYTES - 1;
    is_error(&world.pull(), 413, "Image too large for the update slot");
    assert_eq!(world.fetched().len(), 1);
}

#[test]
fn a_content_length_that_differs_from_the_manifest_is_refused_before_any_write() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(5000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    world.serve(
        ASSET,
        Answer::Body {
            data: sup.clone(),
            length: Some(sup.len() as u64 + 1),
            end: End::Eof,
            tick: 0,
        },
    );
    is_error(
        &world.pull(),
        400,
        "Download size differs from the manifest",
    );
    assert_eq!(world.adapter.count("begin"), 0);
}

#[test]
fn a_wrong_signature_is_refused_before_begin() {
    let mut world = World::new();
    let other = SigningKey::from_bytes(&random_bytes::<32>());
    let sup = file(&other, newer(), &image(5000));
    world.serve_release_file("0.2.0", &sup);
    is_error(&world.pull(), 400, "Invalid signature");
    assert_eq!(world.adapter.count("begin"), 0);
    world.untouched();
}

#[test]
fn a_changed_image_byte_fails_the_signed_hash_and_is_aborted() {
    let mut world = World::new();
    let mut sup = file(&world.signing, newer(), &image(5000));
    let last = sup.len() - 1;
    sup[last] ^= 0xFF;
    // The manifest describes the changed file (size and hash agree with what is served).
    world.serve_release_file("0.2.0", &sup);
    is_error(&world.pull(), 400, "Image does not match its signature");
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_manifest_hash_of_another_file_is_refused_and_finish_never_runs() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(5000));
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    manifest["image"]["sha256"] = json!(hex(&sha256(b"another file")));
    world.serve_manifest(&manifest);
    world.serve(ASSET, body(&sup));
    is_error(&world.pull(), 400, "Download does not match the manifest");
    assert_eq!(world.adapter.count("finish"), 0);
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_body_longer_than_the_manifest_says_is_refused() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(5000));
    let mut longer = sup.clone();
    longer.extend_from_slice(&[1, 2, 3]);
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    world.serve(ASSET, body_no_length(&longer));
    let answer = world.pull();
    assert_eq!(answer.0, 400, "{}", answer.1);
    let message = answer.1["error"].as_str().unwrap();
    assert!(
        message == "Update longer than declared"
            || message == "Download does not match the manifest",
        "{message}"
    );
    assert_eq!(world.adapter.count("finish"), 0);
    world.untouched();
}

#[test]
fn a_header_that_disagrees_with_the_manifest_is_refused_before_begin() {
    let mut world = World::new();
    // The header's length + 128 differs from image.size.
    let sup = file(&world.signing, newer(), &image(5000));
    let mut manifest = manifest_for(&sup, "0.2.0", ASSET);
    manifest["image"]["size"] = json!(sup.len() + 10);
    world.serve_manifest(&manifest);
    world.serve(ASSET, body_no_length(&sup));
    is_error(&world.pull(), 400, "Download does not match the manifest");
    assert_eq!(world.adapter.count("begin"), 0);
    // The header's version differs from the manifest's.
    let mut world = World::new();
    let sup = file(&world.signing, Version::new(0, 3, 0), &image(5000));
    world.serve_release_file("0.2.0", &sup);
    is_error(&world.pull(), 400, "Download does not match the manifest");
    assert_eq!(world.adapter.count("begin"), 0);
}

#[test]
fn a_cut_connection_is_a_download_failure_and_the_next_pull_starts_over() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    world.serve(
        ASSET,
        Answer::Body {
            data: sup[..4000].to_vec(),
            length: Some(sup.len() as u64),
            end: End::Error(ErrorKind::ConnectionReset),
            tick: 0,
        },
    );
    let answer = world.pull();
    assert_eq!(answer.0, 500, "{}", answer.1);
    assert!(
        answer.1["error"]
            .as_str()
            .unwrap()
            .starts_with("Download failed: "),
        "{}",
        answer.1
    );
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
    // The next attempt starts over from byte 0 (the cache is fresh, only the file is fetched).
    world.serve(ASSET, body(&sup));
    world.elapse(2 * MINUTE);
    assert_eq!(world.pull().0, 200);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
}

#[test]
fn a_body_that_ends_early_without_an_error_is_connection_lost() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    world.serve(ASSET, body_no_length(&sup[..5000]));
    is_error(&world.pull(), 500, "Download failed: connection lost");
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_read_timeout_is_a_download_failure() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    world.serve(
        ASSET,
        Answer::Body {
            data: sup[..3000].to_vec(),
            length: Some(sup.len() as u64),
            end: End::Error(ErrorKind::TimedOut),
            tick: 0,
        },
    );
    is_error(&world.pull(), 500, "Download failed: timed out");
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn the_total_time_is_bounded_on_the_monotonic_clock() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(20_000));
    world.serve_manifest(&manifest_for(&sup, "0.2.0", ASSET));
    // Each read takes two seconds: past the ten seconds of the limit before the end.
    world.serve(
        ASSET,
        Answer::Body {
            data: sup.clone(),
            length: Some(sup.len() as u64),
            end: End::Eof,
            tick: 2 * SECOND,
        },
    );
    is_error(&world.pull(), 500, "Download failed: took longer than 10 s");
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_manifest_check_is_bounded_in_time_too() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let text = manifest_for(&sup, "0.2.0", ASSET).to_string();
    world.serve(
        DEFAULT_MANIFEST_URL,
        Answer::Body {
            data: text.into_bytes(),
            length: None,
            end: End::Eof,
            tick: 6 * SECOND,
        },
    );
    is_error(
        &world.check(),
        500,
        "Update check failed: took longer than 5 s",
    );
}

#[test]
fn without_a_key_the_pull_is_503_and_fetches_nothing() {
    let mut world = World::without_key();
    world.serve_release(newer(), 2000);
    is_error(
        &world.pull(),
        503,
        "Updates are not configured: this build has no update key",
    );
    assert!(world.fetched().is_empty());
}

#[test]
fn a_pull_while_a_job_holds_the_ticket_is_409() {
    let world = World::new();
    let first = world.request("POST", UPDATE_PATH, Some(0));
    assert!(matches!(first, Reply::UpdatePull(_)));
    is_error(
        &whole(world.request("POST", UPDATE_PATH, Some(0))),
        409,
        "Update already in progress",
    );
    is_error(
        &whole(world.request("POST", CHECK_PATH, Some(0))),
        409,
        "Update check already in progress",
    );
}

#[test]
fn a_running_image_on_probation_is_409_before_any_fetch() {
    let mut world = World::new();
    world.serve_release(newer(), 2000);
    world.adapter.pending = true;
    is_error(&world.pull(), 409, "The running image is not confirmed yet");
    assert!(world.fetched().is_empty());
}

/// A fetcher that, at its first GET, tries an upload on the same `Updates`.
struct Intruder<'a> {
    inner: FakeFetcher,
    updates: &'a Updates,
    status: Rc<Cell<u16>>,
}

impl Fetcher for Intruder<'_> {
    fn get(&mut self, url: &str, idle_ms: u64) -> Result<Fetched, String> {
        if self.status.get() == 0 {
            let mut other = FakeAdapter::new();
            let response = self
                .updates
                .install(&mut Cursor::new(vec![0u8; 10]), &mut other);
            self.status.set(response.status);
        }
        self.inner.get(url, idle_ms)
    }
}

#[test]
fn an_upload_while_a_pull_holds_the_installation_is_409() {
    let mut world = World::new();
    let sup = world.serve_release(newer(), 2000);
    let Reply::UpdatePull(ticket) = world.request("POST", UPDATE_PATH, Some(0)) else {
        panic!("not a pull")
    };
    let status = Rc::new(Cell::new(0));
    let mut intruder = Intruder {
        inner: FakeFetcher {
            answers: world.fetcher.answers.clone(),
            log: Rc::clone(&world.fetcher.log),
            clock: Rc::clone(&world.clock),
        },
        updates: &world.updates,
        status: Rc::clone(&status),
    };
    let mut pacing = FakePacing {
        clock: Rc::clone(&world.clock),
        pauses: Rc::clone(&world.pauses),
    };
    let response = ticket.pull(
        &world.updates,
        &mut intruder,
        &mut world.adapter,
        &mut pacing,
        CHECK,
        DOWNLOAD,
    );
    assert_eq!(response.status, 200, "{response:?}");
    assert_eq!(status.get(), 409);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
}

#[test]
fn a_failing_check_in_a_pull_is_500_and_the_next_pull_within_a_minute_is_429() {
    let mut world = World::new();
    world.serve(DEFAULT_MANIFEST_URL, Answer::Status(500));
    let answer = world.pull();
    assert_eq!(answer.0, 500);
    assert!(
        answer.1["error"]
            .as_str()
            .unwrap()
            .starts_with("Update check failed: ")
    );
    let fetched = world.fetched().len();
    world.elapse(10 * SECOND);
    let answer = world.pull();
    assert_eq!(answer.0, 429, "{}", answer.1);
    assert_eq!(world.fetched().len(), fetched);
    // A check through the API counts as well.
    world.elapse(MINUTE);
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
    world.elapse(11 * MINUTE);
    assert_eq!(world.pull().0, 200);
}

/// A source that runs a check on its first read: an upload that is being read.
struct Checking<'a> {
    ticket: Option<solite_core::update::release::ReleaseTicket>,
    fetcher: &'a mut FakeFetcher,
    clock: Rc<Cell<u64>>,
    status: Rc<Cell<u16>>,
}

impl Read for Checking<'_> {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        if let Some(ticket) = self.ticket.take() {
            let mut pacing = FakePacing {
                clock: Rc::clone(&self.clock),
                pauses: Rc::new(Cell::new(0)),
            };
            self.status
                .set(ticket.check(self.fetcher, &mut pacing, CHECK).status);
        }
        Ok(0)
    }
}

#[test]
fn a_check_during_an_upload_is_allowed() {
    let mut world = World::new();
    world.serve_release(newer(), 2000);
    let Reply::VersionCheck(ticket) = world.request("POST", CHECK_PATH, Some(0)) else {
        panic!("not a check")
    };
    let status = Rc::new(Cell::new(0));
    let mut source = Checking {
        ticket: Some(ticket),
        fetcher: &mut world.fetcher,
        clock: Rc::clone(&world.clock),
        status: Rc::clone(&status),
    };
    // The upload holds the installation while its source is read; it ends truncated.
    let response = world.updates.install(&mut source, &mut world.adapter);
    assert_eq!(response.status, 400);
    assert_eq!(status.get(), 200);
}

#[test]
fn the_pull_hands_the_cpu_back_about_every_64_kb() {
    let mut world = World::new();
    world.serve_release(newer(), 400 * 1024);
    assert_eq!(world.pull().0, 200);
    let pauses = world.pauses.get();
    assert!((5..=7).contains(&pauses), "{pauses} pauses for 400 KB");
}

// ---- The automatic check ----

fn auto_world(setting: Option<&str>) -> World {
    let world = World::new();
    if let Some(value) = setting {
        world.set_setting("update.autoCheck", value);
    }
    world
}

fn auto(
    world: &World,
    wall: u64,
    mono: u64,
) -> Option<solite_core::update::release::ReleaseTicket> {
    world.home.lock().unwrap().auto_check_ticket(wall, mono)
}

#[test]
fn the_automatic_check_never_runs_when_off() {
    for setting in [None, Some(""), Some("false")] {
        let world = auto_world(setting);
        for mono in [0, 10 * MINUTE, 48 * 60 * MINUTE] {
            assert!(auto(&world, T0, mono).is_none(), "{setting:?} at {mono}");
        }
    }
}

#[test]
fn the_automatic_check_runs_at_ten_minutes_then_every_24_hours() {
    let world = auto_world(Some("true"));
    assert!(auto(&world, T0, 9 * MINUTE).is_none());
    let first = auto(&world, T0, 10 * MINUTE);
    assert!(first.is_some());
    drop(first);
    assert!(auto(&world, T0, 11 * MINUTE).is_none());
    assert!(auto(&world, T0, 10 * MINUTE + 23 * 60 * MINUTE).is_none());
    assert!(auto(&world, T0, 10 * MINUTE + 24 * 60 * MINUTE).is_some());
}

#[test]
fn the_automatic_check_waits_for_the_clock_and_for_a_free_ticket() {
    let world = auto_world(Some("true"));
    // The clock not set: postponed to the next minute.
    assert!(auto(&world, 1_000_000, 10 * MINUTE).is_none());
    assert!(auto(&world, T0, 10 * MINUTE + 30 * SECOND).is_none());
    let ticket = auto(&world, T0, 11 * MINUTE);
    assert!(ticket.is_some());
    drop(ticket);
    // Busy: postponed too.
    let world = auto_world(Some("true"));
    let held = world.request("POST", CHECK_PATH, Some(0));
    assert!(auto(&world, T0, 10 * MINUTE).is_none());
    drop(held);
    assert!(auto(&world, T0, 11 * MINUTE).is_some());
}

#[test]
fn a_busy_ticket_at_the_due_time_asks_again_a_minute_later() {
    let world = auto_world(Some("true"));
    let held = world.request("POST", CHECK_PATH, Some(0));
    assert!(auto(&world, T0, 10 * MINUTE).is_none());
    drop(held);
    assert!(auto(&world, T0, 10 * MINUTE + 30 * SECOND).is_none());
    assert!(auto(&world, T0, 11 * MINUTE).is_some());
}

#[test]
fn a_failed_start_postpones_the_automatic_check_by_a_minute() {
    let world = auto_world(Some("true"));
    let ticket = auto(&world, T0, 10 * MINUTE);
    assert!(ticket.is_some());
    // The thread could not start: the ticket is dropped, the next ask is a minute on.
    drop(ticket);
    world.home.lock().unwrap().auto_check_postpone(10 * MINUTE);
    assert!(auto(&world, T0, 10 * MINUTE + 30 * SECOND).is_none());
    assert!(auto(&world, T0, 11 * MINUTE).is_some());
}

#[test]
fn an_automatic_check_fills_the_cache_and_never_writes() {
    let mut world = auto_world(Some("true"));
    world.serve_release(newer(), 2000);
    let ticket = auto(&world, T0, 10 * MINUTE).unwrap();
    let mut pacing = FakePacing {
        clock: Rc::clone(&world.clock),
        pauses: Rc::clone(&world.pauses),
    };
    ticket.check_auto(&mut world.fetcher, &mut pacing, CHECK);
    let (_, info) = world.version();
    assert_eq!(info["latest"], "0.2.0");
    assert_eq!(info["updateAvailable"], true);
    assert!(world.adapter.calls.is_empty());
    assert_eq!(world.fetched(), vec![DEFAULT_MANIFEST_URL]);
}

#[test]
fn a_failed_automatic_check_does_not_count_for_the_api_interval() {
    let mut world = auto_world(Some("true"));
    world.serve(DEFAULT_MANIFEST_URL, Answer::Status(500));
    let ticket = auto(&world, T0, 10 * MINUTE).unwrap();
    let mut pacing = FakePacing {
        clock: Rc::clone(&world.clock),
        pauses: Rc::clone(&world.pauses),
    };
    ticket.check_auto(&mut world.fetcher, &mut pacing, CHECK);
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
}

// ---- The settings ----

#[test]
fn the_manifest_url_setting_is_checked() {
    let world = World::new();
    let put = |value: &str| {
        let mut entries = Map::new();
        entries.insert("update.manifestUrl".into(), json!(value));
        world.home.lock().unwrap().put_settings(&entries)
    };
    let long_ok = format!("https://example.com/{}", "m".repeat(512 - 20));
    for ok in [
        "https://example.com/m.json",
        "http://192.168.1.5:8000/m.json",
        "",
        &long_ok,
    ] {
        assert!(put(ok).is_ok(), "{ok}");
    }
    let too_long = format!("{long_ok}m");
    for bad in [
        too_long.as_str(),
        "http://example.com/m.json",
        "x",
        "ftp://192.168.1.5/m",
    ] {
        let refused = put(bad).unwrap_err();
        assert_eq!(
            (refused.status, refused.message.as_str()),
            (400, "Invalid value for update.manifestUrl"),
            "{bad}"
        );
    }
}

#[test]
fn the_auto_check_setting_takes_true_false_or_empty() {
    let world = World::new();
    let put = |value: &str| {
        let mut entries = Map::new();
        entries.insert("update.autoCheck".into(), json!(value));
        world.home.lock().unwrap().put_settings(&entries)
    };
    for ok in ["true", "false", ""] {
        assert!(put(ok).is_ok(), "{ok}");
    }
    let refused = put("yes").unwrap_err();
    assert_eq!(
        (refused.status, refused.message.as_str()),
        (400, "Invalid value for update.autoCheck")
    );
}

#[test]
fn the_ticket_uses_the_manifest_url_setting_else_the_default() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    world.serve(
        "http://192.168.1.9:8000/m.json",
        body(manifest_for(&sup, "0.2.0", ASSET).to_string().as_bytes()),
    );
    world.set_setting("update.manifestUrl", "http://192.168.1.9:8000/m.json");
    assert_eq!(world.check().0, 200);
    assert_eq!(world.fetched(), vec!["http://192.168.1.9:8000/m.json"]);
    // Emptied: the default again.
    world.set_setting("update.manifestUrl", "");
    world.elapse(2 * MINUTE);
    world.serve_release(newer(), 2000);
    assert_eq!(world.check().0, 200);
    assert_eq!(world.fetched().last().unwrap(), DEFAULT_MANIFEST_URL);
}

#[test]
fn a_settings_file_without_the_keys_loads_as_before() {
    let (settings, repaired) =
        solite_core::settings::Settings::parse_file(r#"{"home.name":"Annex"}"#).unwrap();
    assert!(!repaired);
    assert_eq!(settings.get("home.name"), Some("Annex"));
    assert_eq!(settings.get("update.manifestUrl"), None);
    assert_eq!(settings.get("update.autoCheck"), None);
}
