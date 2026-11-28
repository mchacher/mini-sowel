//! A release found and pulled by the box (spec 039): the manifest and its rules, where the box
//! may fetch from, the check and pull routes behind the gate, the pull's bounds and failures, the
//! automatic check, the settings. A fake `Fetcher` serves URLs from a map; a fake `Pacing` has a
//! clock the test moves; key pairs are made here from random bytes.

mod support;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
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

const ASSET: &str = "https://github.com/o/r/releases/download/v9.9.0/solite-9.9.0.sup";
const STORE: &str = "https://objects.example.com/signed/solite-9.9.0.sup";
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

/// A release newer than any the tree runs (the running version is the crate's, which every
/// release raises): fixed far above, so these tests need no change at a release.
const NEWER: &str = "9.9.0";

fn newer() -> Version {
    Version::parse(NEWER).unwrap()
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
    /// A 206 (to a request that asked a range): the rest of the file, with its headers.
    Partial {
        data: Vec<u8>,
        content_range: Option<String>,
        length: Option<u64>,
        etag: Option<String>,
        end: End,
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
    /// Counts the bodies dropped (a connection closed).
    dropped: Rc<Cell<u32>>,
}

impl Drop for Script {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
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
    /// The `ETag` of the 200 of these addresses.
    etags: HashMap<String, String>,
    /// What a ranged request of an address gets, in order; once empty, the plain answer of
    /// `answers` (a server that ignores the range).
    script: HashMap<String, VecDeque<Answer>>,
    /// Each ranged request: the address, `from`, and the bodies dropped before it was made.
    range_log: Rc<RefCell<Vec<(String, u64, u32)>>>,
    dropped: Rc<Cell<u32>>,
}

impl FakeFetcher {
    fn new(clock: Rc<Cell<u64>>) -> FakeFetcher {
        FakeFetcher {
            answers: HashMap::new(),
            log: Rc::new(RefCell::new(Vec::new())),
            clock,
            etags: HashMap::new(),
            script: HashMap::new(),
            range_log: Rc::new(RefCell::new(Vec::new())),
            dropped: Rc::new(Cell::new(0)),
        }
    }

    fn script_of(&self, data: Vec<u8>, end: End, tick: u64) -> Box<dyn Read> {
        Box::new(Script {
            data: Cursor::new(data),
            end,
            tick,
            clock: Rc::clone(&self.clock),
            dropped: Rc::clone(&self.dropped),
        })
    }

    fn answer(&self, url: &str, answer: Option<Answer>) -> Result<Fetched, String> {
        match answer {
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
                etag: self.etags.get(url).cloned(),
                body: self.script_of(data, end, tick),
            }),
            Some(Answer::Partial {
                data,
                content_range,
                length,
                etag,
                end,
            }) => Ok(Fetched::Partial {
                content_range,
                length,
                etag,
                body: self.script_of(data, end, 0),
            }),
        }
    }
}

impl Fetcher for FakeFetcher {
    fn get(&mut self, url: &str, _idle_ms: u64) -> Result<Fetched, String> {
        self.log.borrow_mut().push(url.to_string());
        self.answer(url, self.answers.get(url).cloned())
    }

    fn get_range(&mut self, url: &str, from: u64, _idle_ms: u64) -> Result<Fetched, String> {
        self.range_log
            .borrow_mut()
            .push((url.to_string(), from, self.dropped.get()));
        let scripted = self.script.get_mut(url).and_then(VecDeque::pop_front);
        let answer = scripted.or_else(|| self.answers.get(url).cloned());
        self.answer(url, answer)
    }
}

struct FakePacing {
    clock: Rc<Cell<u64>>,
    pauses: Rc<Cell<u32>>,
    waits: Rc<RefCell<Vec<u64>>>,
}

impl Pacing for FakePacing {
    fn now_ms(&mut self) -> u64 {
        self.clock.get()
    }

    fn pause(&mut self) {
        self.pauses.set(self.pauses.get() + 1);
    }

    /// A wait moves the fake clock: nothing sleeps.
    fn wait_ms(&mut self, ms: u64) {
        self.waits.borrow_mut().push(ms);
        self.clock.set(self.clock.get() + ms);
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
    waits: Rc<RefCell<Vec<u64>>>,
    /// The download's limits of `pull` (short ones, a test may widen them).
    download: Limits,
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
            fetcher: FakeFetcher::new(Rc::clone(&clock)),
            clock,
            pauses: Rc::new(Cell::new(0)),
            waits: Rc::new(RefCell::new(Vec::new())),
            download: DOWNLOAD,
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
            waits: Rc::clone(&self.waits),
        };
        let response = match reply {
            Reply::VersionCheck(ticket) => ticket.check(&mut self.fetcher, &mut pacing, CHECK),
            Reply::UpdatePull(ticket) => ticket.pull(
                &self.updates,
                &mut self.fetcher,
                &mut self.adapter,
                &mut pacing,
                CHECK,
                self.download,
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

    /// Queues what the next ranged request of `url` gets.
    fn range(&mut self, url: &str, answer: Answer) {
        self.fetcher
            .script
            .entry(url.to_string())
            .or_default()
            .push_back(answer);
    }

    fn range_log(&self) -> Vec<(String, u64, u32)> {
        self.fetcher.range_log.borrow().clone()
    }

    fn waits(&self) -> Vec<u64> {
        self.waits.borrow().clone()
    }

    /// A release of `len` image bytes at `ASSET`, its first answer cut after `at` bytes of the
    /// file (the length announced is the whole file's). Returns the file.
    fn serve_cut(&mut self, len: usize, at: usize, end: End) -> Vec<u8> {
        let sup = file(&self.signing, newer(), &image(len));
        self.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
        self.serve(
            ASSET,
            Answer::Body {
                data: sup[..at].to_vec(),
                length: Some(sup.len() as u64),
                end,
                tick: 0,
            },
        );
        sup
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

/// A 206 of `sup` from byte `from`, to the end, with its right headers.
fn partial(sup: &[u8], from: usize) -> Answer {
    partial_cut(sup, from, sup.len() - from, End::Eof)
}

/// A 206 from `from` that announces the rest but carries only `carry` bytes, then `end`.
fn partial_cut(sup: &[u8], from: usize, carry: usize, end: End) -> Answer {
    Answer::Partial {
        data: sup[from..from + carry].to_vec(),
        content_range: Some(format!("bytes {from}-{}/{}", sup.len() - 1, sup.len())),
        length: Some((sup.len() - from) as u64),
        etag: None,
        end,
    }
}

/// The download limits when the waits of three resumes (14 s) must fit.
fn long_download() -> Limits {
    Limits {
        idle_ms: 100,
        total_ms: 600 * SECOND,
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
    assert_eq!(info["latest"], NEWER);
    assert_eq!(info["updateAvailable"], true);
    // The kept fields are the ones a pull then uses: size and hash of the file, no re-fetch.
    let (status, done) = world.pull();
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["message"], "Installed 9.9.0, restarting");
    assert_eq!(world.adapter.slot.len(), sup.len() - HEADER_BYTES);
}

#[test]
fn a_manifest_of_another_format_is_refused() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
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
    let good = manifest_for(&sup, NEWER, ASSET);
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
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
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
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
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
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    let (_, info) = world.check();
    assert_eq!(
        info["releaseUrl"],
        format!("https://github.com/o/r/releases/tag/v{NEWER}")
    );
    world.serve_manifest(&manifest_for(
        &sup,
        NEWER,
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
    world.serve_manifest(&manifest_for(&sup, NEWER, url));
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
    let manifest = manifest_for(&sup, NEWER, ASSET).to_string();
    world.serve(
        DEFAULT_MANIFEST_URL,
        redirect("https://github.com/mchacher/solite/releases/download/v9.9.0/manifest.json"),
    );
    world.serve(
        "https://github.com/mchacher/solite/releases/download/v9.9.0/manifest.json",
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
    assert_eq!(world.version().1["latest"], NEWER);
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
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    let (status, info) = world.check();
    assert_eq!(status, 200, "{info}");
    assert_eq!(
        info,
        json!({
            "current": solite_core::VERSION,
            "latest": NEWER,
            "updateAvailable": true,
            "releaseUrl": format!("https://github.com/o/r/releases/tag/v{NEWER}"),
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
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
    manifest["minVersion"] = json!("9.9.9");
    world.serve_manifest(&manifest);
    let (_, info) = world.check();
    assert_eq!(info["updateAvailable"], false);
    assert_eq!(info["latest"], NEWER);
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
        json!({"success": true, "message": "Installed 9.9.0, restarting"})
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
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
    manifest["minVersion"] = json!(running().to_string());
    world.serve_manifest(&manifest);
    assert_eq!(world.pull().0, 200);
}

#[test]
fn an_image_above_the_slot_is_413() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(2000));
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
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
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
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
    world.serve_release_file(NEWER, &sup);
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
    world.serve_release_file(NEWER, &sup);
    is_error(&world.pull(), 400, "Image does not match its signature");
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_manifest_hash_of_another_file_is_refused_and_finish_never_runs() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(5000));
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
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
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
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
    let mut manifest = manifest_for(&sup, NEWER, ASSET);
    manifest["image"]["size"] = json!(sup.len() + 10);
    world.serve_manifest(&manifest);
    world.serve(ASSET, body_no_length(&sup));
    is_error(&world.pull(), 400, "Download does not match the manifest");
    assert_eq!(world.adapter.count("begin"), 0);
    // The header's version differs from the manifest's.
    let mut world = World::new();
    let sup = file(&world.signing, Version::new(9, 8, 0), &image(5000));
    world.serve_release_file(NEWER, &sup);
    is_error(&world.pull(), 400, "Download does not match the manifest");
    assert_eq!(world.adapter.count("begin"), 0);
}

#[test]
fn a_server_that_does_not_resume_fails_the_pull_and_the_next_pull_starts_over() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    world.serve(
        ASSET,
        Answer::Body {
            data: sup[..4000].to_vec(),
            length: Some(sup.len() as u64),
            end: End::Error(ErrorKind::ConnectionReset),
            tick: 0,
        },
    );
    // The same cut answer comes back to the range request (a 200): the server does not resume.
    is_error(
        &world.pull(),
        500,
        "Download failed: the server does not resume (200 to a range request)",
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
fn a_body_that_ends_early_without_an_error_is_a_cut_too() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    world.serve(ASSET, body_no_length(&sup[..5000]));
    // The resumed request is answered with the same short 200: nothing to resume from.
    world.download = Limits {
        idle_ms: 100,
        total_ms: 600 * SECOND,
    };
    is_error(
        &world.pull(),
        500,
        "Download failed: the server does not resume (200 to a range request)",
    );
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_read_timeout_every_time_is_a_download_failure_after_the_resumes() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    world.serve(
        ASSET,
        Answer::Body {
            data: sup[..3000].to_vec(),
            length: Some(sup.len() as u64),
            end: End::Error(ErrorKind::TimedOut),
            tick: 0,
        },
    );
    for _ in 0..3 {
        world.range(
            ASSET,
            partial_cut(&sup, 3000, 0, End::Error(ErrorKind::TimedOut)),
        );
    }
    world.download = long_download();
    is_error(
        &world.pull(),
        500,
        "Download failed: timed out (after 3 resumes)",
    );
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn the_total_time_is_bounded_on_the_monotonic_clock() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(20_000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
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
    let text = manifest_for(&sup, NEWER, ASSET).to_string();
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
        inner: {
            let mut inner = FakeFetcher::new(Rc::clone(&world.clock));
            inner.answers = world.fetcher.answers.clone();
            inner.log = Rc::clone(&world.fetcher.log);
            inner
        },
        updates: &world.updates,
        status: Rc::clone(&status),
    };
    let mut pacing = FakePacing {
        clock: Rc::clone(&world.clock),
        pauses: Rc::clone(&world.pauses),
        waits: Rc::clone(&world.waits),
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
                waits: Rc::default(),
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

// ---- A cut download resumed (spec 040 R6) ----

/// The log lines of this file's tests (warnings and up), to see a resume and a check.
struct Capture;
static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if let Ok(mut lines) = LINES.lock() {
            lines.push(format!("{}: {}", record.level(), record.args()));
        }
    }
    fn flush(&self) {}
}

fn capture_logs() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = log::set_logger(&Capture);
        log::set_max_level(log::LevelFilter::Info);
    });
}

/// The captured lines that hold this text (a number or an address unique to the test).
fn logged(text: &str) -> Vec<String> {
    LINES
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.contains(text))
        .cloned()
        .collect()
}

#[test]
fn a_cut_download_resumes_from_the_byte_written_and_installs() {
    capture_logs();
    let mut world = World::new();
    let sup = world.serve_cut(8101, 4000, End::Error(ErrorKind::ConnectionReset));
    world.range(ASSET, partial(&sup, 4000));
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
    assert_eq!(world.adapter.count("begin"), 1);
    assert_eq!(world.adapter.count("abort"), 0);
    // One request, asking the rest from the byte read, made after two bodies were dropped (the
    // manifest's, the cut one); one wait of 2 s on the fake clock.
    assert_eq!(world.range_log(), vec![(ASSET.to_string(), 4000, 2)]);
    assert_eq!(world.waits(), vec![2000]);
    let lines = logged(&format!("download cut at 4000 of {} bytes", sup.len()));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("INFO: update: download cut at"),
        "{lines:?}"
    );
    assert!(lines[0].contains("(scripted), resuming (1/3)"), "{lines:?}");
}

#[test]
fn the_cut_body_is_dropped_before_the_new_request() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    world.range(ASSET, partial(&sup, 4000));
    assert_eq!(world.pull().0, 200);
    // By the time the ranged request is made, the manifest's body and the cut one are dropped.
    let log = world.range_log();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].2, 2);
}

#[test]
fn two_cuts_are_two_resumes() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 3000, End::Eof);
    world.range(ASSET, partial_cut(&sup, 3000, 2500, End::Eof));
    world.range(ASSET, partial(&sup, 5500));
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
    let froms: Vec<u64> = world.range_log().iter().map(|(_, from, _)| *from).collect();
    assert_eq!(froms, vec![3000, 5500]);
    assert_eq!(world.waits(), vec![2000, 4000]);
}

#[test]
fn a_cut_at_byte_zero_of_the_body_asks_from_zero_and_installs() {
    let mut world = World::new();
    let sup = world.serve_cut(6000, 0, End::Error(ErrorKind::ConnectionReset));
    world.range(ASSET, partial(&sup, 0));
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(world.range_log()[0].1, 0);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
}

#[test]
fn a_cut_inside_the_header_resumes_like_any_other_byte() {
    let mut world = World::new();
    let sup = world.serve_cut(6000, 60, End::Eof);
    world.range(ASSET, partial(&sup, 60));
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(world.range_log()[0].1, 60);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
}

#[test]
fn a_read_timeout_is_resumed() {
    let mut world = World::new();
    let sup = world.serve_cut(6007, 3000, End::Error(ErrorKind::TimedOut));
    world.range(ASSET, partial(&sup, 3000));
    capture_logs();
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
    let lines = logged(&format!(
        "download cut at 3000 of {} bytes (timed out)",
        sup.len()
    ));
    assert_eq!(lines.len(), 1, "{lines:?}");
}

#[test]
fn a_200_to_the_range_request_is_a_server_that_does_not_resume() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    world.range(ASSET, body(&sup));
    is_error(
        &world.pull(),
        500,
        "Download failed: the server does not resume (200 to a range request)",
    );
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_416_is_a_changed_file_and_the_cached_manifest_is_dropped() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    world.range(ASSET, Answer::Status(416));
    is_error(
        &world.pull(),
        500,
        "Download failed: the file changed on the server",
    );
    world.untouched();
    // The next pull reads the manifest again (without the drop, the cache would still be fresh).
    world.serve(ASSET, body(&sup));
    world.elapse(2 * MINUTE - 1);
    let manifests = |world: &World| {
        world
            .fetched()
            .iter()
            .filter(|url| *url == DEFAULT_MANIFEST_URL)
            .count()
    };
    assert_eq!(manifests(&world), 1);
    // 2 minutes on the monotonic clock is past the 60 s of the checks and the cache stays
    // fresh (under 10 minutes): only the drop makes the next pull fetch the manifest.
    assert_eq!(world.pull().0, 200);
    assert_eq!(manifests(&world), 2);
}

#[test]
fn a_206_that_is_not_the_rest_asked_for_is_a_changed_file() {
    type Bend = fn(&mut Answer);
    let bends: [(&str, Bend); 5] = [
        ("another start", |a| {
            if let Answer::Partial { content_range, .. } = a {
                *content_range = Some(content_range.clone().unwrap().replacen("4000", "3999", 1));
            }
        }),
        ("another total", |a| {
            if let Answer::Partial { content_range, .. } = a {
                let text = content_range.clone().unwrap();
                let (head, _) = text.rsplit_once('/').unwrap();
                *content_range = Some(format!("{head}/99999"));
            }
        }),
        ("a short end", |a| {
            if let Answer::Partial { content_range, .. } = a {
                let text = content_range.clone().unwrap();
                let (head, total) = text.rsplit_once('/').unwrap();
                let (start, _) = head.split_once('-').unwrap();
                *content_range = Some(format!("{start}-5000/{total}"));
            }
        }),
        ("no Content-Range", |a| {
            if let Answer::Partial { content_range, .. } = a {
                *content_range = None;
            }
        }),
        ("a wrong Content-Length", |a| {
            if let Answer::Partial { length, .. } = a {
                *length = length.map(|n| n - 1);
            }
        }),
    ];
    for (what, bend) in bends {
        let mut world = World::new();
        let sup = world.serve_cut(8000, 4000, End::Eof);
        let mut answer = partial(&sup, 4000);
        bend(&mut answer);
        world.range(ASSET, answer);
        let result = world.pull();
        assert_eq!(result.0, 500, "{what}: {}", result.1);
        assert_eq!(
            result.1["error"], "Download failed: the file changed on the server",
            "{what}"
        );
        assert_eq!(world.adapter.count("abort"), 1, "{what}");
        world.untouched();
    }
}

#[test]
fn a_changed_etag_between_the_two_answers_is_a_changed_file() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    world
        .fetcher
        .etags
        .insert(ASSET.to_string(), "\"v1\"".to_string());
    let mut answer = partial(&sup, 4000);
    if let Answer::Partial { etag, .. } = &mut answer {
        *etag = Some("\"v2\"".to_string());
    }
    world.range(ASSET, answer);
    is_error(
        &world.pull(),
        500,
        "Download failed: the file changed on the server",
    );
    world.untouched();
}

#[test]
fn an_etag_missing_on_one_answer_or_over_256_characters_is_not_compared() {
    let long = "x".repeat(300);
    // (first answer, 206): absent on the 206, absent on the first, a long one that differs.
    let cases: [(Option<&str>, Option<&str>); 3] = [
        (Some("\"v1\""), None),
        (None, Some("\"v2\"")),
        (Some(long.as_str()), Some("\"other\"")),
    ];
    for (first, second) in cases {
        let mut world = World::new();
        let sup = world.serve_cut(8000, 4000, End::Eof);
        if let Some(first) = first {
            world
                .fetcher
                .etags
                .insert(ASSET.to_string(), first.to_string());
        }
        let mut answer = partial(&sup, 4000);
        if let Answer::Partial { etag, .. } = &mut answer {
            *etag = second.map(str::to_string);
        }
        world.range(ASSET, answer);
        let result = world.pull();
        assert_eq!(result.0, 200, "{first:?} {second:?}: {}", result.1);
        assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
    }
    // The same etag on both answers is accepted too.
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    world
        .fetcher
        .etags
        .insert(ASSET.to_string(), "\"v1\"".to_string());
    let mut answer = partial(&sup, 4000);
    if let Answer::Partial { etag, .. } = &mut answer {
        *etag = Some("\"v1\"".to_string());
    }
    world.range(ASSET, answer);
    assert_eq!(world.pull().0, 200);
}

#[test]
fn a_changed_file_served_without_an_etag_is_caught_by_the_final_checks() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    // The same length, other bytes after the cut.
    let mut other = sup.clone();
    other[4000..]
        .iter_mut()
        .for_each(|b| *b = b.wrapping_add(1));
    world.range(ASSET, partial(&other, 4000));
    // The signature's image digest is checked before the manifest's file hash: the bytes after
    // the cut do not match what the header signed.
    is_error(&world.pull(), 400, "Image does not match its signature");
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
}

#[test]
fn a_read_error_at_the_full_count_is_the_end_of_the_body() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(5000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    world.serve(
        ASSET,
        Answer::Body {
            data: sup.clone(),
            length: Some(sup.len() as u64),
            end: End::Error(ErrorKind::ConnectionReset),
            tick: 0,
        },
    );
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert!(world.range_log().is_empty());
    assert!(world.waits().is_empty());
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
}

#[test]
fn a_503_on_the_range_request_counts_as_one_attempt() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    world.range(ASSET, Answer::Status(503));
    world.range(ASSET, partial(&sup, 4000));
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(world.range_log().len(), 2);
    assert_eq!(world.waits(), vec![2000, 4000]);
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
}

#[test]
fn a_resumed_request_that_cannot_connect_counts_as_one_attempt() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 4000, End::Eof);
    world.range(ASSET, Answer::Fail("connection refused".to_string()));
    world.range(ASSET, partial(&sup, 4000));
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(world.waits(), vec![2000, 4000]);
}

#[test]
fn any_other_status_on_the_range_request_fails_at_once() {
    let mut world = World::new();
    world.serve_cut(8000, 4000, End::Eof);
    world.range(ASSET, Answer::Status(403));
    is_error(
        &world.pull(),
        500,
        &format!("Download failed: {ASSET}: 403"),
    );
    assert_eq!(world.range_log().len(), 1);
    world.untouched();
}

#[test]
fn a_redirect_to_plain_http_during_a_resume_fails_at_once() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    world.serve(ASSET, redirect(STORE));
    world.serve(
        STORE,
        Answer::Body {
            data: sup[..4000].to_vec(),
            length: Some(sup.len() as u64),
            end: End::Eof,
            tick: 0,
        },
    );
    world.range(ASSET, redirect("http://objects.example.com/plain.sup"));
    is_error(
        &world.pull(),
        500,
        "Download failed: redirect from https to http refused",
    );
    assert_eq!(world.range_log().len(), 1);
    assert_eq!(world.waits(), vec![2000]);
    world.untouched();
}

#[test]
fn a_206_to_the_first_download_request_or_to_the_manifest_is_a_status() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(3000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    world.serve(ASSET, partial(&sup, 0));
    is_error(
        &world.pull(),
        500,
        &format!("Download failed: {ASSET}: 206"),
    );
    world.untouched();

    let mut world = World::new();
    world.serve(DEFAULT_MANIFEST_URL, partial(&sup, 0));
    is_error(
        &world.check(),
        500,
        &format!("Update check failed: {DEFAULT_MANIFEST_URL}: 206"),
    );
}

#[test]
fn every_attempt_cut_fails_after_three_resumes_with_waits_of_2_4_and_8_s() {
    let mut world = World::new();
    let sup = world.serve_cut(8000, 3000, End::Eof);
    world.range(ASSET, partial_cut(&sup, 3000, 500, End::Eof));
    world.range(ASSET, partial_cut(&sup, 3500, 500, End::Eof));
    world.range(ASSET, partial_cut(&sup, 4000, 500, End::Eof));
    world.download = long_download();
    is_error(
        &world.pull(),
        500,
        "Download failed: connection lost (after 3 resumes)",
    );
    assert_eq!(world.waits(), vec![2000, 4000, 8000]);
    let froms: Vec<u64> = world.range_log().iter().map(|(_, from, _)| *from).collect();
    assert_eq!(froms, vec![3000, 3500, 4000]);
    assert_eq!(world.adapter.count("abort"), 1);
    world.untouched();
    // Nothing is kept across pulls: the next one starts over from byte 0.
    world.serve(ASSET, body(&sup));
    world.elapse(2 * MINUTE);
    assert_eq!(world.pull().0, 200);
    assert_eq!(world.range_log().len(), 3);
}

#[test]
fn the_total_time_wins_over_a_resume() {
    let mut world = World::new();
    world.serve_cut(8000, 3000, End::Eof);
    // The first wait (2 s) is already past a total of 1 s.
    world.download = Limits {
        idle_ms: 100,
        total_ms: SECOND,
    };
    is_error(&world.pull(), 500, "Download failed: took longer than 1 s");
    assert!(world.range_log().is_empty());
    world.untouched();
}

#[test]
fn each_range_request_starts_at_the_image_url_and_follows_its_redirects() {
    let mut world = World::new();
    let sup = file(&world.signing, newer(), &image(8000));
    world.serve_manifest(&manifest_for(&sup, NEWER, ASSET));
    world.serve(ASSET, redirect(STORE));
    world.serve(
        STORE,
        Answer::Body {
            data: sup[..4000].to_vec(),
            length: Some(sup.len() as u64),
            end: End::Eof,
            tick: 0,
        },
    );
    // The resume asks the asset address again (no remembered signed address), then the store.
    world.range(STORE, partial(&sup, 4000));
    let answer = world.pull();
    assert_eq!(answer.0, 200, "{}", answer.1);
    assert_eq!(
        world.range_log(),
        vec![(ASSET.to_string(), 4000, 2), (STORE.to_string(), 4000, 2)]
    );
    assert_eq!(world.adapter.slot, sup[HEADER_BYTES..]);
}

// ---- Observations from the box (spec 040 R7 a) ----

#[test]
fn a_fresh_home_has_no_latest_and_a_check_always_reaches_the_fetcher() {
    capture_logs();
    let mut world = World::new();
    let (status, info) = world.version();
    assert_eq!(status, 200, "{info}");
    assert!(info["latest"].is_null(), "{info}");
    // The address is this test's own: the success line names it.
    let url = "http://192.168.1.77:8081/spec040-observation-a.json";
    let sup = file(&world.signing, newer(), &image(2000));
    world.serve(
        url,
        body(manifest_for(&sup, NEWER, ASSET).to_string().as_bytes()),
    );
    world.set_setting("update.manifestUrl", url);
    assert_eq!(world.check().0, 200);
    assert_eq!(world.fetched(), vec![url.to_string()]);
    // The cache is filled: a second check, past the 60 s, still fetches.
    world.elapse(2 * MINUTE);
    assert_eq!(world.check().0, 200);
    assert_eq!(world.fetched(), vec![url.to_string(), url.to_string()]);
    let lines = logged(&format!(
        "update: check: latest 9.9.0, running {} ({url})",
        solite_core::VERSION
    ));
    assert_eq!(lines.len(), 2, "{lines:?}");
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
        waits: Rc::clone(&world.waits),
    };
    ticket.check_auto(&mut world.fetcher, &mut pacing, CHECK);
    let (_, info) = world.version();
    assert_eq!(info["latest"], NEWER);
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
        waits: Rc::clone(&world.waits),
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
        body(manifest_for(&sup, NEWER, ASSET).to_string().as_bytes()),
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
