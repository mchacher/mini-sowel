//! The account, its password and its sessions (spec 029): the hash, `auth.json`, sessions and
//! attempts, the presence adapters, Sowel's `/auth` routes and the gate through
//! `api::handle_guarded`, the factory reset, `bearer_token`, and the saver and loader.

mod mem_storage;
mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mem_storage::MemStorage;
use serde_json::{Value, json};
use solite_core::api::{Reach, Reply, Request, handle, handle_guarded};
use solite_core::auth::{
    ACCESS_TTL_S, Account, AlwaysConfirmed, Attempts, BOOT_CONFIRM_MS, BootPresence,
    ButtonPresence, CONFIRM_WINDOW_MS, Language, MAX_ATTEMPTS, MAX_SESSIONS, NeverConfirmed,
    PBKDF2_ITERATIONS, PasswordHash, Presence, REFRESH_TTL_S, Sessions, hashes_on_this_thread,
    parse_file, pbkdf2_sha256, set_hash_hook, to_file_json,
};
use solite_core::devices::{Id, format_iso, lock};
use solite_core::engine::{AUTH_FILE, DEFINITION_DELAY_MS, RETRY_MS, Saver, load_home};
use solite_core::home::Home;
use solite_core::live::session::bearer_token;
use support::{HOST, PORT, Random, Rig, T0, counter};

const PASSWORD: &str = "correct horse battery";
const PAIRING: &str = "/api/v1/integrations/solite/permit-join";

// ---- A server: a home behind its lock, called through `handle_guarded` ----

struct Server {
    home: Arc<Mutex<Home>>,
    now: Cell<u64>,
    random: RefCell<Random>,
    fail_random: Cell<bool>,
}

fn salt(n: u8) -> [u8; 16] {
    [n; 16]
}

fn account(username: &str, password: &str) -> Account {
    Account {
        id: Id::parse("11111111-1111-4111-8111-111111111111").unwrap(),
        username: username.into(),
        display_name: "Owner".into(),
        language: Language::En,
        created_at: T0,
        password: PasswordHash::new(password, salt(7)),
    }
}

impl Server {
    /// A fresh home where a person at the box confirms.
    fn new() -> Server {
        Server::with_home(Home::new(T0))
    }

    fn with_home(mut home: Home) -> Server {
        home.set_presence(AlwaysConfirmed);
        Server {
            home: Arc::new(Mutex::new(home)),
            now: Cell::new(T0),
            random: RefCell::new(counter()),
            fail_random: Cell::new(false),
        }
    }

    /// A server whose home has the account `admin` with `PASSWORD` (a hash made here).
    fn with_account() -> Server {
        let server = Server::new();
        lock(&server.home).load_auth(Some(account("admin", PASSWORD)));
        server
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        authorization: Option<&str>,
        content_type: Option<&str>,
        body: &str,
        host: Option<&str>,
    ) -> Reply {
        let mut random = |buffer: &mut [u8]| -> Result<(), String> {
            if self.fail_random.get() {
                return Err("no entropy".into());
            }
            (self.random.borrow_mut())(buffer)
        };
        handle_guarded(
            &self.home,
            &Request {
                method,
                path,
                host,
                port: PORT,
                reach: Reach::Loopback,
                content_type,
                authorization,
                content_length: None,
                body: body.as_bytes(),
            },
            self.now.get(),
            &mut random,
        )
    }

    /// (status, body as JSON, `Null` without one); the lists are drained.
    fn send(
        &self,
        method: &str,
        path: &str,
        authorization: Option<&str>,
        content_type: Option<&str>,
        body: &str,
    ) -> (u16, Value) {
        let reply = self.request(method, path, authorization, content_type, body, Some(HOST));
        match reply {
            Reply::Whole(response) | Reply::StaticError(response) => (
                response.status,
                response
                    .body
                    .map_or(Value::Null, |t| serde_json::from_str(&t).unwrap()),
            ),
            Reply::List(mut cursor) => {
                let mut text = String::new();
                while let Some(chunk) = cursor.next_chunk(&lock(&self.home)).unwrap() {
                    text.push_str(&chunk);
                }
                (200, serde_json::from_str(&text).unwrap())
            }
            Reply::Static(_) => (200, Value::Null),
            Reply::Order(_)
            | Reply::Update
            | Reply::UpdatePull(_)
            | Reply::VersionCheck(_)
            | Reply::BackupExport(_)
            | Reply::BackupRestore { .. } => (202, Value::Null),
        }
    }

    fn json(&self, method: &str, path: &str, token: Option<&str>, body: Value) -> (u16, Value) {
        let header = token.map(|t| format!("Bearer {t}"));
        self.send(
            method,
            path,
            header.as_deref(),
            Some("application/json"),
            &body.to_string(),
        )
    }

    fn get(&self, path: &str, token: Option<&str>) -> (u16, Value) {
        let header = token.map(|t| format!("Bearer {t}"));
        self.send("GET", path, header.as_deref(), None, "")
    }

    fn setup(&self) -> Value {
        let (status, body) = self.json(
            "POST",
            "/api/v1/auth/setup",
            None,
            json!({"username": "admin", "password": PASSWORD, "displayName": "Owner"}),
        );
        assert_eq!(status, 201, "{body}");
        body
    }

    fn login(&self, username: &str, password: &str) -> (u16, Value) {
        self.json(
            "POST",
            "/api/v1/auth/login",
            None,
            json!({"username": username, "password": password}),
        )
    }

    fn access(&self) -> String {
        let (status, body) = self.login("admin", PASSWORD);
        assert_eq!(status, 200, "{body}");
        body["accessToken"].as_str().unwrap().to_string()
    }
}

fn error(body: &Value) -> &str {
    body["error"].as_str().unwrap_or("")
}

// ---- The hash ----

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn pbkdf2_hmac_sha256_gives_the_known_vectors() {
    // RFC 7914 section 11 / the usual PBKDF2-HMAC-SHA256 vectors, 32 bytes.
    assert_eq!(
        hex(&pbkdf2_sha256(b"password", b"salt", 1)),
        "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
    );
    assert_eq!(
        hex(&pbkdf2_sha256(b"password", b"salt", 4096)),
        "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
    );
}

#[test]
fn a_hash_verifies_the_right_password_and_nothing_near_it() {
    let hash = PasswordHash::new("hunter2hunter2", salt(1));
    assert_eq!(hash.iterations, PBKDF2_ITERATIONS);
    assert!(hash.verify("hunter2hunter2"));
    assert!(!hash.verify("hunter2hunter3"));
    assert!(!hash.verify("hunter2hunter"));
    assert!(!hash.verify(""));
}

#[test]
fn two_hashes_of_one_password_with_two_salts_differ() {
    let a = PasswordHash::new(PASSWORD, salt(1));
    let b = PasswordHash::new(PASSWORD, salt(2));
    assert_ne!(a.hash, b.hash);
    assert_ne!(a.salt, b.salt);
}

// ---- auth.json ----

#[test]
fn an_account_written_then_parsed_is_equal_and_holds_no_password() {
    let written = account("admin", PASSWORD);
    let text = to_file_json(Some(&written));
    assert_eq!(parse_file(&text).unwrap(), Some(written));
    assert!(!text.contains(PASSWORD));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["password"]["scheme"], "pbkdf2-sha256");
    assert_eq!(value["password"]["iterations"], 10_000);
    assert_eq!(value["password"]["salt"].as_str().unwrap().len(), 32);
    assert_eq!(value["password"]["hash"].as_str().unwrap().len(), 64);
    assert_eq!(value["user"]["language"], "en");
}

#[test]
fn no_account_is_a_file_with_only_its_version() {
    assert_eq!(to_file_json(None), r#"{"version":1}"#);
    assert_eq!(parse_file(r#"{"version":1}"#).unwrap(), None);
}

fn file_with(change: impl FnOnce(&mut Value)) -> String {
    let mut value: Value =
        serde_json::from_str(&to_file_json(Some(&account("admin", "x")))).unwrap();
    change(&mut value);
    value.to_string()
}

fn load_error(text: &str) -> String {
    let storage = MemStorage::with(&[(AUTH_FILE, text)]);
    load_home(&&storage, T0).expect_err("the start is stopped")
}

#[test]
fn a_file_that_is_not_valid_stops_the_start() {
    let cases = [
        "not json".to_string(),
        "".to_string(),
        r#"{"version":2}"#.to_string(),
        file_with(|v| v["password"]["scheme"] = "bcrypt".into()),
        file_with(|v| v["password"]["iterations"] = 999.into()),
        file_with(|v| v["password"]["iterations"] = 1_000_001.into()),
        file_with(|v| v["password"]["salt"] = "ab".repeat(15).into()),
        file_with(|v| v["password"]["hash"] = "ab".repeat(31).into()),
        file_with(|v| v["password"]["hash"] = "AB".repeat(32).into()),
        file_with(|v| {
            v["user"].as_object_mut().unwrap().remove("id");
        }),
        file_with(|v| v["user"]["username"] = "x".repeat(65).into()),
        file_with(|v| v["user"]["username"] = "bad\nname".into()),
        file_with(|v| v["user"]["username"] = "".into()),
        file_with(|v| v["user"]["displayName"] = "".into()),
        file_with(|v| v["user"]["language"] = "de".into()),
        file_with(|v| {
            v.as_object_mut().unwrap().remove("password");
        }),
        // An account under another key must never read as "no account" (R1.4).
        file_with(|v| {
            let user = v.as_object_mut().unwrap().remove("user").unwrap();
            v["account"] = user;
        }),
        file_with(|v| v["user"]["role"] = "admin".into()),
        file_with(|v| v["password"]["pepper"] = "x".into()),
    ];
    for text in cases {
        let message = load_error(&text);
        assert!(
            message.starts_with("auth: memory://auth.json: cannot read: "),
            "{text}: {message}"
        );
    }
}

#[test]
fn a_home_without_auth_json_asks_for_the_setup_and_keeps_its_home() {
    let mut rig = Rig::new();
    let zone = rig.zone("Garage");
    let storage = MemStorage::default();
    let home = Mutex::new(rig.home);
    Saver::new(&storage, T0).tick(&home, T0, true).unwrap();
    assert!(
        storage
            .text(AUTH_FILE)
            .is_some_and(|t| t == r#"{"version":1}"#)
            || storage.text(AUTH_FILE).is_none()
    );
    storage.files.borrow_mut().remove(AUTH_FILE);
    let loaded = load_home(&&storage, T0).unwrap();
    assert!(loaded.setup_required());
    assert!(loaded.zones().find(&zone).is_some());
    assert!(!loaded.auth_changed());
    let server = Server::with_home(loaded);
    assert_eq!(
        server.get("/api/v1/auth/status", None),
        (200, json!({"setupRequired": true}))
    );
}

#[test]
fn a_file_with_no_account_reads_as_no_account() {
    let storage = MemStorage::with(&[(AUTH_FILE, r#"{"version":1}"#)]);
    assert!(load_home(&&storage, T0).unwrap().setup_required());
}

// ---- Sessions ----

const A: [u8; 16] = [0xab; 16];
const R: [u8; 16] = [2; 16];

fn hex16(bytes: [u8; 16]) -> String {
    hex(&bytes)
}

#[test]
fn an_access_token_is_valid_for_900_seconds_and_not_a_millisecond_more() {
    let mut sessions = Sessions::new();
    sessions.open(A, R, 1_000);
    let end = 1_000 + ACCESS_TTL_S * 1000;
    assert!(sessions.access_valid(&hex16(A), 1_000));
    assert!(sessions.access_valid(&hex16(A), end));
    assert!(!sessions.access_valid(&hex16(A), end + 1));
    // The refresh token is not an access token.
    assert!(!sessions.access_valid(&hex16(R), 1_000));
}

#[test]
fn a_refresh_token_lives_30_days_and_works_once() {
    let mut sessions = Sessions::new();
    sessions.open(A, R, 0);
    assert!(!sessions.take_refresh(&hex16(R), REFRESH_TTL_S * 1000 + 1));
    sessions.open(A, R, 0);
    assert!(sessions.take_refresh(&hex16(R), REFRESH_TTL_S * 1000));
    // Rotation: the old session is gone, its access token with it.
    assert!(!sessions.access_valid(&hex16(A), 1));
    assert!(!sessions.take_refresh(&hex16(R), 1));
}

#[test]
fn the_ninth_session_drops_the_first() {
    let mut sessions = Sessions::new();
    for n in 0..=MAX_SESSIONS {
        let n = u8::try_from(n).unwrap();
        sessions.open([n + 10; 16], [n + 100; 16], 1_000);
    }
    assert_eq!(sessions.len(), MAX_SESSIONS);
    assert!(!sessions.access_valid(&hex16([10; 16]), 1_000));
    assert!(sessions.access_valid(&hex16([11; 16]), 1_000));
    assert!(sessions.access_valid(&hex16([18; 16]), 1_000));
}

#[test]
fn a_token_of_the_wrong_shape_is_invalid() {
    let mut sessions = Sessions::new();
    sessions.open(A, R, 1_000);
    let good = hex16(A);
    assert!(sessions.access_valid(&good, 1_000));
    for bad in [
        &good[..31],
        &format!("{good}0"),
        &good.to_uppercase(),
        &"zz".repeat(16),
        "",
        " ",
    ] {
        assert!(!sessions.access_valid(bad, 1_000), "{bad:?}");
    }
}

#[test]
fn a_session_taken_before_the_clock_was_set_moves_with_the_jump() {
    let mut sessions = Sessions::new();
    sessions.open(A, R, 5_000);
    sessions.shift_before_clock(T0 - 5_000);
    assert!(sessions.access_valid(&hex16(A), T0 + 1_000));
    assert!(!sessions.access_valid(&hex16(A), T0 + ACCESS_TTL_S * 1000 + 1));
    // A session taken after the clock was set does not move.
    let mut sessions = Sessions::new();
    sessions.open(A, R, T0);
    sessions.shift_before_clock(10_000_000);
    assert!(!sessions.access_valid(&hex16(A), T0 + ACCESS_TTL_S * 1000 + 1));
}

#[test]
fn a_session_opened_after_now_is_expired_and_dropped() {
    let mut sessions = Sessions::new();
    sessions.open(A, R, 10_000);
    assert!(!sessions.access_valid(&hex16(A), 9_999));
    sessions.drop_after(9_999);
    assert!(sessions.is_empty());
}

// ---- Attempts ----

#[test]
fn ten_attempts_in_a_minute_are_accepted_the_eleventh_is_not() {
    let mut attempts = Attempts::new();
    for i in 0..MAX_ATTEMPTS {
        assert!(attempts.allow(T0 + i as u64), "{i}");
    }
    assert!(!attempts.allow(T0 + 10));
    assert!(!attempts.allow(T0 + 59_999));
    assert_eq!(attempts.len(), MAX_ATTEMPTS);
    // The first attempt is a minute old at T0 + 60 000.
    assert!(attempts.allow(T0 + 60_000));
}

#[test]
fn an_attempt_dated_after_now_is_dropped_and_never_blocks() {
    let mut attempts = Attempts::new();
    for _ in 0..MAX_ATTEMPTS {
        assert!(attempts.allow(T0 + 1_000_000));
    }
    assert!(!attempts.allow(T0 + 1_000_001));
    // The clock went back: the attempts of the future are gone.
    assert!(attempts.allow(T0));
    assert_eq!(attempts.len(), 1);
}

// ---- Presence ----

fn clock(at: &Arc<AtomicU64>) -> Arc<dyn Fn() -> u64 + Send + Sync> {
    let at = Arc::clone(at);
    Arc::new(move || at.load(Ordering::SeqCst))
}

#[test]
fn a_boot_presence_confirms_for_ten_minutes_after_a_power_on_only() {
    let uptime = Arc::new(AtomicU64::new(0));
    let power_on = BootPresence::new(true, clock(&uptime));
    let restart = BootPresence::new(false, clock(&uptime));
    for (at, confirmed) in [
        (0, true),
        (BOOT_CONFIRM_MS - 1, true),
        (BOOT_CONFIRM_MS, false),
    ] {
        uptime.store(at, Ordering::SeqCst);
        assert_eq!(power_on.confirmed(), confirmed, "{at}");
        assert!(!restart.confirmed(), "{at}");
    }
}

#[test]
fn a_button_presence_confirms_until_two_minutes_after_the_press() {
    let uptime = Arc::new(AtomicU64::new(1_000));
    let button = ButtonPresence::new(clock(&uptime));
    assert!(!button.confirmed());
    let held = button.clone();
    button.press();
    // A clone shares the press.
    for (at, confirmed) in [
        (1_000, true),
        (1_000 + CONFIRM_WINDOW_MS - 1, true),
        (1_000 + CONFIRM_WINDOW_MS, false),
    ] {
        uptime.store(at, Ordering::SeqCst);
        assert_eq!(held.confirmed(), confirmed, "{at}");
    }
    // The uptime never goes back, but a press "in the future" confirms nothing.
    uptime.store(10, Ordering::SeqCst);
    assert!(!held.confirmed());
}

#[test]
fn the_default_presence_never_confirms_and_says_so() {
    let presence = NeverConfirmed;
    assert!(!presence.confirmed());
    assert_eq!(presence.hint(), "Confirmation at the box is not available");
    assert!(AlwaysConfirmed.confirmed());
}

// ---- /auth/status and setup ----

#[test]
fn the_status_tells_whether_the_setup_is_required() {
    let server = Server::new();
    assert_eq!(
        server.get("/api/v1/auth/status", None),
        (200, json!({"setupRequired": true}))
    );
    server.setup();
    assert_eq!(
        server.get("/api/v1/auth/status", None),
        (200, json!({"setupRequired": false}))
    );
}

#[test]
fn a_setup_makes_the_account_a_session_and_marks_auth_json_to_write() {
    let server = Server::new();
    assert!(!lock(&server.home).auth_changed());
    let body = server.setup();
    assert_eq!(body["expiresIn"], 900);
    assert_eq!(body["accessToken"].as_str().unwrap().len(), 32);
    assert_eq!(body["refreshToken"].as_str().unwrap().len(), 32);
    let user = &body["user"];
    assert_eq!(user["username"], "admin");
    assert_eq!(user["displayName"], "Owner");
    assert_eq!(user["role"], "admin");
    assert_eq!(user["enabled"], true);
    assert_eq!(user["preferences"], json!({"language": "fr"}));
    assert_eq!(user["createdAt"], format_iso(T0));
    assert_eq!(user["lastLoginAt"], format_iso(T0));
    assert!(user["id"].as_str().is_some());
    assert!(lock(&server.home).auth_changed());
    // The token works.
    let token = body["accessToken"].as_str().unwrap();
    assert_eq!(server.get("/api/v1/zones", Some(token)).0, 200);
}

#[test]
fn a_setup_without_a_language_is_french_and_takes_en() {
    let server = Server::new();
    assert_eq!(server.setup()["user"]["preferences"]["language"], "fr");
    let english = Server::new();
    let (status, body) = english.json(
        "POST",
        "/api/v1/auth/setup",
        None,
        json!({"username": "a", "password": PASSWORD, "displayName": "A", "language": "en"}),
    );
    assert_eq!(status, 201);
    assert_eq!(body["user"]["preferences"]["language"], "en");
}

#[test]
fn a_setup_with_an_account_is_403_before_its_body_is_read() {
    let server = Server::new();
    server.setup();
    for body in [
        json!({"username": "b", "password": PASSWORD, "displayName": "B"}),
        json!({"nonsense": 1}),
    ] {
        let (status, answer) = server.json("POST", "/api/v1/auth/setup", None, body);
        assert_eq!((status, error(&answer)), (403, "Setup already completed"));
    }
    let (status, answer) = server.send(
        "POST",
        "/api/v1/auth/setup",
        None,
        Some("application/json"),
        "not json",
    );
    assert_eq!((status, error(&answer)), (403, "Setup already completed"));
}

#[test]
fn a_setup_without_the_presence_is_403_with_its_hint_and_makes_nothing() {
    let server = Server::new();
    lock(&server.home).set_presence(ButtonPresence::new(Arc::new(|| 0)));
    let (status, body) = server.json(
        "POST",
        "/api/v1/auth/setup",
        None,
        json!({"username": "admin", "password": PASSWORD, "displayName": "Owner"}),
    );
    assert_eq!(status, 403);
    assert_eq!(
        body,
        json!({"error": "Press the button on the box, then try again", "confirmRequired": true})
    );
    assert!(lock(&server.home).setup_required());
    assert!(!lock(&server.home).auth_changed());
}

#[test]
fn each_presence_gives_its_own_hint_in_the_refusal() {
    let hints: Vec<(Box<dyn Presence>, &str)> = vec![
        (
            Box::new(NeverConfirmed),
            "Confirmation at the box is not available",
        ),
        (
            Box::new(BootPresence::new(false, Arc::new(|| 0))),
            "Unplug the box and plug it back in, then try again within 10 minutes",
        ),
        (
            Box::new(ButtonPresence::new(Arc::new(|| 0))),
            "Press the button on the box, then try again",
        ),
    ];
    for (presence, hint) in hints {
        struct Boxed(Box<dyn Presence>);
        impl Presence for Boxed {
            fn confirmed(&self) -> bool {
                self.0.confirmed()
            }
            fn hint(&self) -> &str {
                self.0.hint()
            }
        }
        let server = Server::new();
        lock(&server.home).set_presence(Boxed(presence));
        let (status, body) = server.json(
            "POST",
            "/api/v1/auth/setup",
            None,
            json!({"username": "admin", "password": PASSWORD, "displayName": "Owner"}),
        );
        assert_eq!((status, error(&body)), (403, hint));
        assert_eq!(body["confirmRequired"], true);
    }
}

#[test]
fn a_bad_setup_body_is_a_400_each() {
    let ok = |change: &dyn Fn(&mut Value)| {
        let mut body = json!({"username": "admin", "password": PASSWORD, "displayName": "Owner"});
        change(&mut body);
        body
    };
    let cases: Vec<(Value, &str)> = vec![
        (
            ok(&|b| b["password"] = "1234567".into()),
            "Invalid field: password",
        ),
        (
            ok(&|b| b["password"] = "x".repeat(129).into()),
            "Invalid field: password",
        ),
        (ok(&|b| b["password"] = "x".repeat(128).into()), ""),
        // 4 emoji: 16 bytes but 4 characters; 8 emoji are 8 characters; 129 emoji are 516 bytes.
        (
            ok(&|b| b["password"] = "😀".repeat(4).into()),
            "Invalid field: password",
        ),
        (ok(&|b| b["password"] = "😀".repeat(8).into()), ""),
        (
            ok(&|b| b["password"] = "😀".repeat(129).into()),
            "Invalid field: password",
        ),
        (ok(&|b| b["password"] = "é".repeat(100).into()), ""),
        (
            ok(&|b| b["password"] = "".into()),
            "Invalid field: password",
        ),
        (
            ok(&|b| b["password"] = 12345678.into()),
            "Invalid field: password",
        ),
        (
            ok(&|b| b["username"] = "x".repeat(65).into()),
            "Invalid field: username",
        ),
        (
            ok(&|b| b["username"] = "a\nb".into()),
            "Invalid field: username",
        ),
        (
            ok(&|b| b["username"] = "".into()),
            "Invalid field: username",
        ),
        (
            ok(&|b| b["displayName"] = "x".repeat(65).into()),
            "Invalid field: displayName",
        ),
        (
            ok(&|b| {
                b.as_object_mut().unwrap().remove("displayName");
            }),
            "Missing field: displayName",
        ),
        (
            ok(&|b| {
                b.as_object_mut().unwrap().remove("username");
            }),
            "Missing field: username",
        ),
        (ok(&|b| b["extra"] = 1.into()), "Unknown field: extra"),
        (
            ok(&|b| b["language"] = "de".into()),
            "Invalid field: language",
        ),
    ];
    for (body, expected) in cases {
        let server = Server::new();
        let (status, answer) = server.json("POST", "/api/v1/auth/setup", None, body.clone());
        if expected.is_empty() {
            assert_eq!(status, 201, "{body}");
        } else {
            assert_eq!((status, error(&answer)), (400, expected), "{body}");
            assert!(lock(&server.home).setup_required());
        }
    }
    let server = Server::new();
    let (status, answer) = server.send(
        "POST",
        "/api/v1/auth/setup",
        None,
        Some("application/json"),
        "[1]",
    );
    assert_eq!((status, error(&answer)), (400, "Invalid JSON"));
}

#[test]
fn two_setups_at_once_make_one_account_and_the_other_is_403() {
    let server = Server::new();
    // While the first setup hashes (the lock is free), a second one runs whole.
    let nested: Rc<RefCell<Option<(u16, Value)>>> = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&nested);
    let home = Arc::clone(&server.home);
    set_hash_hook(Some(Box::new(move || {
        if slot.borrow().is_some() {
            return;
        }
        *slot.borrow_mut() = Some((0, Value::Null));
        let other = Server {
            home: Arc::clone(&home),
            now: Cell::new(T0),
            random: RefCell::new(counter()),
            fail_random: Cell::new(false),
        };
        let answer = other.json(
            "POST",
            "/api/v1/auth/setup",
            None,
            json!({"username": "second", "password": PASSWORD, "displayName": "Second"}),
        );
        *slot.borrow_mut() = Some(answer);
    })));
    let (status, body) = server.json(
        "POST",
        "/api/v1/auth/setup",
        None,
        json!({"username": "first", "password": PASSWORD, "displayName": "First"}),
    );
    set_hash_hook(None);
    // The nested one finished first and made the account; the outer one is the second to finish.
    let (nested_status, nested_body) = nested.borrow().clone().unwrap();
    assert_eq!(nested_status, 201, "{nested_body}");
    assert_eq!((status, error(&body)), (403, "Setup already completed"));
    assert_eq!(lock(&server.home).account().unwrap().username, "second");
}

#[test]
fn the_home_is_not_locked_while_a_password_is_hashed() {
    let server = Server::with_account();
    let free = Rc::new(Cell::new(None));
    let seen = Rc::clone(&free);
    let home = Arc::clone(&server.home);
    set_hash_hook(Some(Box::new(move || {
        seen.set(Some(home.try_lock().is_ok()));
    })));
    let (status, _) = server.login("admin", PASSWORD);
    set_hash_hook(None);
    assert_eq!(status, 200);
    assert_eq!(free.get(), Some(true), "the lock is free during the hash");
    // The setup too.
    let server = Server::new();
    let free = Rc::new(Cell::new(None));
    let seen = Rc::clone(&free);
    let home = Arc::clone(&server.home);
    set_hash_hook(Some(Box::new(move || {
        seen.set(Some(home.try_lock().is_ok()));
    })));
    server.setup();
    set_hash_hook(None);
    assert_eq!(free.get(), Some(true));
}

// ---- login, refresh, logout ----

#[test]
fn a_login_with_the_right_pair_answers_the_tokens() {
    let server = Server::with_account();
    let (status, body) = server.login("admin", PASSWORD);
    assert_eq!(status, 200);
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["preferences"]["language"], "en");
    assert_eq!(body["expiresIn"], 900);
    let token = body["accessToken"].as_str().unwrap();
    assert_eq!(server.get("/api/v1/devices", Some(token)).0, 200);
    // Sowel's `trustedDeviceToken` is accepted and ignored.
    let (status, _) = server.json(
        "POST",
        "/api/v1/auth/login",
        None,
        json!({"username": "admin", "password": PASSWORD, "trustedDeviceToken": "x"}),
    );
    assert_eq!(status, 200);
}

#[test]
fn a_wrong_password_a_wrong_user_and_no_account_are_the_same_401() {
    let server = Server::with_account();
    for (user, password) in [("admin", "wrong password"), ("nobody", PASSWORD)] {
        let (status, body) = server.login(user, password);
        assert_eq!((status, error(&body)), (401, "Invalid credentials"));
    }
    // Each cost a hash, the wrong user name too.
    let before = hashes_on_this_thread();
    server.login("nobody", PASSWORD);
    assert_eq!(hashes_on_this_thread(), before + 1);
    let none = Server::new();
    let before = hashes_on_this_thread();
    let (status, body) = none.login("admin", PASSWORD);
    assert_eq!((status, error(&body)), (401, "Invalid credentials"));
    assert_eq!(hashes_on_this_thread(), before + 1);
}

#[test]
fn the_eleventh_attempt_in_a_minute_is_429_and_hashes_nothing() {
    let server = Server::with_account();
    for _ in 0..10 {
        assert_eq!(server.login("admin", "wrong password").0, 401);
    }
    let before = hashes_on_this_thread();
    let (status, body) = server.login("admin", PASSWORD);
    assert_eq!(
        (status, error(&body)),
        (429, "Rate limit exceeded, retry in 1 minute")
    );
    assert_eq!(hashes_on_this_thread(), before, "no hash computed");
    // A setup counts with the logins.
    let (status, _) = server.json(
        "POST",
        "/api/v1/auth/setup",
        None,
        json!({"username": "a", "password": PASSWORD, "displayName": "A"}),
    );
    assert_eq!(status, 403);
    // After a minute the limit is lifted.
    server.now.set(T0 + 60_000);
    assert_eq!(server.login("admin", PASSWORD).0, 200);
}

#[test]
fn what_does_not_hash_is_not_counted_toward_the_limit() {
    let server = Server::new();
    lock(&server.home).set_presence(NeverConfirmed);
    // 400, 403 (presence) and 415: twenty of each.
    for _ in 0..20 {
        let (status, _) = server.json("POST", "/api/v1/auth/setup", None, json!({"x": 1}));
        assert_eq!(status, 400);
        let (status, _) = server.json(
            "POST",
            "/api/v1/auth/setup",
            None,
            json!({"username": "admin", "password": PASSWORD, "displayName": "Owner"}),
        );
        assert_eq!(status, 403);
        let (status, _) = server.send("POST", "/api/v1/auth/setup", None, None, "{}");
        assert_eq!(status, 415);
    }
    lock(&server.home).set_presence(AlwaysConfirmed);
    server.setup();
    for _ in 0..9 {
        assert_eq!(server.login("admin", "wrong password").0, 401);
    }
    // 1 setup + 9 logins = 10: the next one is refused.
    assert_eq!(server.login("admin", PASSWORD).0, 429);
    // Bad bodies of the login are not counted either.
    server.now.set(T0 + 60_000);
    for _ in 0..20 {
        let (status, _) = server.json("POST", "/api/v1/auth/login", None, json!({"username": 1}));
        assert_eq!(status, 400);
    }
    assert_eq!(server.login("admin", PASSWORD).0, 200);
}

#[test]
fn a_refresh_rotates_the_session() {
    let server = Server::with_account();
    let (_, first) = server.login("admin", PASSWORD);
    let refresh = first["refreshToken"].as_str().unwrap();
    let (status, second) = server.json(
        "POST",
        "/api/v1/auth/refresh",
        None,
        json!({"refreshToken": refresh}),
    );
    assert_eq!(status, 200);
    assert_ne!(second["accessToken"], first["accessToken"]);
    assert_ne!(second["refreshToken"], first["refreshToken"]);
    // The old pair is closed.
    let old = first["accessToken"].as_str().unwrap();
    assert_eq!(server.get("/api/v1/zones", Some(old)).0, 401);
    let new = second["accessToken"].as_str().unwrap();
    assert_eq!(server.get("/api/v1/zones", Some(new)).0, 200);
    // Used twice: the second is refused, and so is an unknown one.
    for token in [refresh, &"0".repeat(32), "short"] {
        let (status, body) = server.json(
            "POST",
            "/api/v1/auth/refresh",
            None,
            json!({"refreshToken": token}),
        );
        assert_eq!(
            (status, error(&body)),
            (401, "Invalid refresh token"),
            "{token}"
        );
    }
    let (status, body) = server.json("POST", "/api/v1/auth/refresh", None, json!({}));
    assert_eq!((status, error(&body)), (400, "Missing field: refreshToken"));
}

#[test]
fn a_refresh_token_sent_as_a_bearer_is_401() {
    let server = Server::with_account();
    let (_, tokens) = server.login("admin", PASSWORD);
    let refresh = tokens["refreshToken"].as_str().unwrap();
    let (status, body) = server.get("/api/v1/zones", Some(refresh));
    assert_eq!((status, error(&body)), (401, "Invalid or expired token"));
}

#[test]
fn a_logout_closes_the_session_and_its_access_token() {
    let server = Server::with_account();
    let (_, tokens) = server.login("admin", PASSWORD);
    let access = tokens["accessToken"].as_str().unwrap();
    let refresh = tokens["refreshToken"].as_str().unwrap();
    let (status, _) = server.json(
        "POST",
        "/api/v1/auth/logout",
        Some(access),
        json!({"refreshToken": refresh}),
    );
    assert_eq!(status, 204);
    assert_eq!(server.get("/api/v1/zones", Some(access)).0, 401);
    let (status, _) = server.json(
        "POST",
        "/api/v1/auth/refresh",
        None,
        json!({"refreshToken": refresh}),
    );
    assert_eq!(status, 401);
}

#[test]
fn a_logout_needs_the_token_and_takes_an_empty_or_unknown_refresh_token() {
    let server = Server::with_account();
    let (status, body) = server.json("POST", "/api/v1/auth/logout", None, json!({}));
    assert_eq!((status, error(&body)), (401, "Authentication required"));
    let access = server.access();
    for body in [json!({}), json!({"refreshToken": "0".repeat(32)})] {
        let (status, _) = server.json("POST", "/api/v1/auth/logout", Some(&access), body);
        assert_eq!(status, 204);
    }
    // Nothing was closed.
    assert_eq!(server.get("/api/v1/zones", Some(&access)).0, 200);
    // No body at all is `{}`.
    let header = format!("Bearer {access}");
    let (status, _) = server.send(
        "POST",
        "/api/v1/auth/logout",
        Some(&header),
        Some("application/json"),
        "",
    );
    assert_eq!(status, 204);
}

#[test]
fn last_login_is_the_last_login_since_the_start() {
    let server = Server::with_account();
    assert_eq!(lock(&server.home).last_login_at(), None);
    server.now.set(T0 + 5_000);
    let (_, body) = server.login("admin", PASSWORD);
    assert_eq!(body["user"]["lastLoginAt"], format_iso(T0 + 5_000));
    // A refresh keeps it.
    server.now.set(T0 + 9_000);
    let refresh = body["refreshToken"].as_str().unwrap();
    let (_, next) = server.json(
        "POST",
        "/api/v1/auth/refresh",
        None,
        json!({"refreshToken": refresh}),
    );
    assert_eq!(next["user"]["lastLoginAt"], format_iso(T0 + 5_000));
    // After a reload of the home, `null`.
    let server = Server::with_account();
    assert_eq!(lock(&server.home).last_login_at(), None);
}

#[test]
fn a_random_source_that_fails_makes_no_account_and_no_session() {
    let server = Server::new();
    server.fail_random.set(true);
    let (status, body) = server.json(
        "POST",
        "/api/v1/auth/setup",
        None,
        json!({"username": "admin", "password": PASSWORD, "displayName": "Owner"}),
    );
    assert_eq!((status, error(&body)), (500, "Internal error"));
    assert!(lock(&server.home).setup_required());
    assert!(!lock(&server.home).auth_changed());
    server.fail_random.set(false);
    server.setup();

    let server = Server::with_account();
    server.fail_random.set(true);
    let (status, body) = server.login("admin", PASSWORD);
    assert_eq!((status, error(&body)), (500, "Internal error"));
    assert_eq!(lock(&server.home).last_login_at(), None);
    server.fail_random.set(false);
    assert_eq!(server.login("admin", PASSWORD).0, 200);
}

// ---- The gate ----

#[test]
fn with_no_account_every_route_but_the_public_ones_is_403_setup_required() {
    let server = Server::new();
    let expected = json!({"error": "Setup required", "setupRequired": true});
    for (method, path) in [
        ("GET", "/api/v1/devices"),
        ("GET", "/api/v1/zones"),
        ("POST", "/api/v1/zones"),
        ("GET", "/api/v1/nothing-here"),
        ("POST", "/api/v1/auth/logout"),
        ("POST", "/api/v1/system/factory-reset"),
        ("GET", PAIRING),
    ] {
        let (status, body) = server.send(method, path, None, None, "");
        assert_eq!((status, &body), (403, &expected), "{method} {path}");
    }
}

#[test]
fn the_gate_answers_in_sowels_order() {
    let server = Server::with_account();
    let access = server.access();
    let wrong = "0".repeat(32);
    let cases: Vec<(Option<String>, &str)> = vec![
        (None, "Authentication required"),
        (Some("Basic abc".into()), "Authentication required"),
        (Some("Bearer ".into()), "Authentication required"),
        (Some("Bearer".into()), "Authentication required"),
        (Some(format!("Bearer {wrong}")), "Invalid or expired token"),
        (Some("Bearer nonsense".into()), "Invalid or expired token"),
    ];
    for (header, message) in cases {
        let (status, body) = server.send("GET", "/api/v1/devices", header.as_deref(), None, "");
        assert_eq!((status, error(&body)), (401, message), "{header:?}");
    }
    // An expired token.
    server.now.set(T0 + ACCESS_TTL_S * 1000 + 1);
    let (status, body) = server.get("/api/v1/devices", Some(&access));
    assert_eq!((status, error(&body)), (401, "Invalid or expired token"));
}

#[test]
fn with_the_token_a_route_answers_and_an_unknown_path_is_404() {
    let server = Server::with_account();
    let access = server.access();
    assert_eq!(
        server.get("/api/v1/devices", Some(&access)),
        (200, json!([]))
    );
    let (status, body) = server.get("/api/v1/x", Some(&access));
    assert_eq!((status, error(&body)), (404, "Not found"));
    let (status, _) = server.get("/api/v1/x", None);
    assert_eq!(status, 401);
    // A path outside the API is the router's 404, whatever the token.
    let (status, _) = server.get("/elsewhere", None);
    assert_eq!(status, 404);
}

#[test]
fn the_gate_comes_before_the_body_the_413_and_the_415() {
    let server = Server::with_account();
    // An unauthenticated POST without a content type: 401, not 415.
    let (status, body) = server.send("POST", "/api/v1/zones", None, None, "{}");
    assert_eq!((status, error(&body)), (401, "Authentication required"));
    let big = "x".repeat(20_000);
    let (status, _) = server.send(
        "POST",
        "/api/v1/zones",
        None,
        Some("application/json"),
        &big,
    );
    assert_eq!(status, 401);
    // With the token the order of the routes is as before.
    let access = server.access();
    let header = format!("Bearer {access}");
    let (status, _) = server.send("POST", "/api/v1/zones", Some(&header), None, "{}");
    assert_eq!(status, 415);
    let (status, _) = server.send(
        "POST",
        "/api/v1/zones",
        Some(&header),
        Some("application/json"),
        &big,
    );
    assert_eq!(status, 413);
}

#[test]
fn a_setup_with_an_account_and_a_bad_body_is_403_not_400() {
    let server = Server::with_account();
    let (status, body) = server.send(
        "POST",
        "/api/v1/auth/setup",
        None,
        Some("application/json"),
        "nonsense",
    );
    assert_eq!((status, error(&body)), (403, "Setup already completed"));
}

#[test]
fn the_interface_needs_no_token_and_a_foreign_host_is_refused_first() {
    let server = Server::with_account();
    let (status, _) = server.get("/", None);
    assert!(matches!(status, 200 | 404), "{status}");
    let reply = server.request("GET", "/", None, None, "", Some(HOST));
    assert!(matches!(reply, Reply::Static(_) | Reply::StaticError(_)));
    for path in ["/", "/api/v1/devices", "/api/v1/auth/status"] {
        let reply = server.request("GET", path, None, None, "", Some("evil.example:8080"));
        match reply {
            Reply::Whole(response) => {
                assert_eq!(response.status, 403, "{path}");
                assert_eq!(
                    response.body.as_deref(),
                    Some(r#"{"error":"Forbidden host"}"#)
                );
            }
            other => panic!("{path}: {other:?}"),
        }
    }
}

#[test]
fn a_wrong_method_on_a_public_route_is_405() {
    let server = Server::new();
    assert_eq!(server.get("/api/v1/auth/login", None).0, 405);
    let (status, _) = server.json("POST", "/api/v1/auth/status", None, json!({}));
    assert_eq!(status, 405);
}

#[test]
fn the_pairing_route_answers_on_the_lan_with_the_token() {
    let server = Server::with_account();
    let access = server.access();
    let header = format!("Bearer {access}");
    let lan = |authorization: Option<&str>, method: &str, body: &str| {
        let reply = handle_guarded(
            &server.home,
            &Request {
                method,
                path: PAIRING,
                host: Some("192.168.1.20"),
                port: 80,
                reach: Reach::Lan {
                    name: "solite-a1b2c3",
                },
                content_type: Some("application/json"),
                authorization,
                content_length: None,
                body: body.as_bytes(),
            },
            T0,
            &mut counter(),
        );
        match reply {
            Reply::Whole(r) => r.status,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(lan(Some(&header), "GET", ""), 200);
    assert_eq!(lan(Some(&header), "POST", r#"{"time":30}"#), 202);
    assert_eq!(lan(None, "GET", ""), 401);
}

// ---- The clock ----

#[test]
fn a_session_opened_before_the_clock_was_set_lives_on_after_the_jump() {
    let server = Server::new();
    server.now.set(5_000);
    let body = server.setup();
    let access = body["accessToken"].as_str().unwrap();
    assert_eq!(server.get("/api/v1/zones", Some(access)).0, 200);
    // `auth.json` was taken by the saver meanwhile.
    assert!(lock(&server.home).take_auth_cursor().is_some());
    assert!(!lock(&server.home).auth_changed());
    // The clock is set: the session lives on (15 minutes from its opening, moved), the account's
    // `createdAt` moves, `auth.json` is to be written again.
    server.now.set(T0 + 1_000);
    assert_eq!(server.get("/api/v1/zones", Some(access)).0, 200);
    assert_eq!(lock(&server.home).account().unwrap().created_at, T0 + 1_000);
    assert!(lock(&server.home).auth_changed());
    server.now.set(T0 + 1_000 + ACCESS_TTL_S * 1000 + 1);
    assert_eq!(server.get("/api/v1/zones", Some(access)).0, 401);
}

#[test]
fn attempts_and_sessions_dated_after_now_never_block() {
    let server = Server::with_account();
    server.now.set(T0 + 10_000_000);
    for _ in 0..10 {
        server.login("admin", "wrong password");
    }
    assert_eq!(server.login("admin", PASSWORD).0, 429);
    let (_, tokens) = {
        server.now.set(T0 + 10_000_000 + 60_000);
        server.login("admin", PASSWORD)
    };
    let access = tokens["accessToken"].as_str().unwrap().to_string();
    // The clock goes back before all of it.
    server.now.set(T0);
    assert_eq!(server.get("/api/v1/zones", Some(&access)).0, 401);
    assert_eq!(server.login("admin", PASSWORD).0, 200);
}

// ---- The saver and the loader ----

#[test]
fn auth_json_is_written_once_after_the_definition_delay_and_loaded_back() {
    let storage = MemStorage::default();
    let home = load_home(&&storage, T0).unwrap();
    let server = Server::with_home(home);
    let mut saver = Saver::new(&storage, T0).with_definition_delay(DEFINITION_DELAY_MS);
    saver.tick(&server.home, T0, false).unwrap();
    storage.per_file.borrow_mut().clear();
    server.setup();
    saver.tick(&server.home, T0, false).unwrap();
    saver
        .tick(&server.home, T0 + DEFINITION_DELAY_MS - 1, false)
        .unwrap();
    assert_eq!(storage.writes(AUTH_FILE), 0);
    saver
        .tick(&server.home, T0 + DEFINITION_DELAY_MS, false)
        .unwrap();
    assert_eq!(storage.writes(AUTH_FILE), 1);
    // Logins write nothing.
    for _ in 0..3 {
        server.login("admin", PASSWORD);
    }
    saver
        .tick(&server.home, T0 + 10 * DEFINITION_DELAY_MS, false)
        .unwrap();
    assert_eq!(storage.writes(AUTH_FILE), 1);
    // A restart keeps the account, ends the sessions.
    let (_, tokens) = server.login("admin", PASSWORD);
    let old = tokens["accessToken"].as_str().unwrap().to_string();
    let restarted = Server::with_home(load_home(&&storage, T0).unwrap());
    assert_eq!(restarted.get("/api/v1/zones", Some(&old)).0, 401);
    assert_eq!(restarted.login("admin", PASSWORD).0, 200);
    assert_eq!(lock(&restarted.home).account().unwrap().username, "admin");
}

#[test]
fn a_failed_write_of_auth_json_is_tried_again_at_the_retry() {
    let storage = MemStorage::default();
    let server = Server::with_home(load_home(&&storage, T0).unwrap());
    let mut saver = Saver::new(&storage, T0);
    saver.tick(&server.home, T0, false).unwrap();
    storage.per_file.borrow_mut().clear();
    *storage.fail_file.borrow_mut() = Some(AUTH_FILE.to_string());
    server.setup();
    saver.tick(&server.home, T0, false).unwrap();
    assert_eq!(storage.writes(AUTH_FILE), 1);
    assert!(storage.text(AUTH_FILE).is_none());
    saver.tick(&server.home, T0 + RETRY_MS - 1, false).unwrap();
    assert_eq!(storage.writes(AUTH_FILE), 1);
    *storage.fail_file.borrow_mut() = None;
    saver.tick(&server.home, T0 + RETRY_MS, false).unwrap();
    assert_eq!(storage.writes(AUTH_FILE), 2);
    assert!(storage.text(AUTH_FILE).unwrap().contains("pbkdf2-sha256"));
}

// ---- The factory reset ----

const RESET: &str = "/api/v1/system/factory-reset";

/// A server whose home is a garage with a recipe instance, and the account set up.
fn garage_server() -> (Server, String) {
    let mut rig = Rig::new();
    let garage = rig.garage();
    rig.garage_instance(&garage, json!({}));
    let (status, _) = rig.put(
        "/api/v1/settings",
        json!({"home.name": "Chez moi", "home.latitude": "48.85", "home.longitude": "2.35"}),
    );
    assert_eq!(status, 200);
    let sub = rig.post(
        "/api/v1/zones",
        json!({"name": "Sub", "parentId": garage.zone}),
    );
    assert_eq!(sub.0, 201, "{:?}", sub.1);
    let (status, _) = rig.put(
        &format!("/api/v1/zones/{ROOT}"),
        json!({"name": "Renamed", "description": "text"}),
    );
    assert_eq!(status, 200);
    let server = Server::with_home(rig.home);
    let access = server.setup()["accessToken"].as_str().unwrap().to_string();
    (server, access)
}

const ROOT: &str = support::ROOT;

#[test]
fn a_factory_reset_erases_the_home_and_the_account() {
    let (server, access) = garage_server();
    let (status, body) = server.json("POST", RESET, Some(&access), json!({}));
    assert_eq!((status, body), (204, Value::Null));
    {
        let mut home = lock(&server.home);
        assert!(home.devices.list().is_empty());
        assert!(home.equipments().list().is_empty());
        assert_eq!(home.recipes().instances().count(), 0);
        assert_eq!(home.zones().list().len(), 1);
        let root = home.zones().get(&Id::ROOT_ZONE).unwrap();
        assert_eq!(root.name, "Maison");
        assert_eq!(root.description, None);
        assert_eq!(root.icon.as_deref(), Some("home"));
        assert_eq!(home.settings().count(), 0);
        assert!(home.setup_required());
        assert_eq!(home.auth_json(), r#"{"version":1}"#);
        assert!(home.take_auth_cursor().is_some());
        assert!(home.changes().0 && home.changes().1);
    }
    // The old token: the API is back at its first start.
    let (status, body) = server.get("/api/v1/zones", Some(&access));
    assert_eq!((status, error(&body)), (403, "Setup required"));
    assert_eq!(
        server.get("/api/v1/auth/status", None),
        (200, json!({"setupRequired": true}))
    );
    // And the home sets up again.
    server.setup();
}

#[test]
fn a_factory_reset_sends_the_events_of_the_deletions_and_writes_auth_json() {
    let (server, access) = garage_server();
    let hub = lock(&server.home).live();
    let id = hub.connect().unwrap();
    let topics =
        r#"{"type":"subscribe","topics":["devices","zones","equipments","recipes","system"]}"#;
    assert!(hub.subscribe(id, topics));
    let storage = MemStorage::default();
    let mut saver = Saver::new(&storage, T0);
    saver.tick(&server.home, T0, false).unwrap();
    assert_eq!(server.json("POST", RESET, Some(&access), json!({})).0, 204);
    let text = hub.take_batch(id).expect("events");
    let events: Vec<Value> = serde_json::from_str(&text).unwrap();
    let count = |name: &str| events.iter().filter(|e| e["type"] == name).count();
    assert_eq!(count("recipe.instance.removed"), 1);
    assert_eq!(count("equipment.removed"), 2);
    assert_eq!(count("device.removed"), 2);
    assert_eq!(count("zone.removed"), 2, "the garage and its sub-zone");
    assert_eq!(count("settings.changed"), 1);
    // The recipe first, then the equipments, the devices, the zones.
    let position = |name: &str| events.iter().position(|e| e["type"] == name).unwrap();
    assert!(position("recipe.instance.removed") < position("equipment.removed"));
    assert!(position("equipment.removed") < position("device.removed"));
    assert!(position("device.removed") < position("zone.removed"));
    saver.tick(&server.home, T0 + 1, false).unwrap();
    assert_eq!(storage.text(AUTH_FILE).as_deref(), Some(r#"{"version":1}"#));
    assert_eq!(storage.text("settings.json").as_deref(), Some("{}"));
}

#[test]
fn a_factory_reset_without_the_presence_or_during_pairing_erases_nothing() {
    let (server, access) = garage_server();
    lock(&server.home).set_presence(NeverConfirmed);
    let (status, body) = server.json("POST", RESET, Some(&access), json!({}));
    assert_eq!(status, 403);
    assert_eq!(body["confirmRequired"], true);
    assert_eq!(error(&body), "Confirmation at the box is not available");
    lock(&server.home).set_presence(AlwaysConfirmed);
    // The window open, then closed with a device still being set up.
    lock(&server.home).pairing_mut().opened(u64::MAX);
    let (status, body) = server.json("POST", RESET, Some(&access), json!({}));
    assert_eq!((status, error(&body)), (409, "Pairing in progress"));
    lock(&server.home).pairing_mut().ended(u64::MAX);
    let (status, body) = server.json("POST", RESET, Some(&access), json!({}));
    assert_eq!((status, error(&body)), (409, "Pairing in progress"));
    assert_eq!(lock(&server.home).devices.list().len(), 2);
    assert!(!lock(&server.home).setup_required());
    // Closed: it goes.
    lock(&server.home).pairing_mut().finished();
    assert_eq!(server.json("POST", RESET, Some(&access), json!({})).0, 204);
}

#[test]
fn a_factory_reset_needs_the_token_and_a_json_body() {
    let (server, access) = garage_server();
    let (status, body) = server.send("POST", RESET, None, None, "");
    assert_eq!((status, error(&body)), (401, "Authentication required"));
    let header = format!("Bearer {access}");
    // 415 after the gate.
    let (status, _) = server.send("POST", RESET, Some(&header), None, "");
    assert_eq!(status, 415);
    let (status, body) = server.json("POST", RESET, Some(&access), json!({"all": true}));
    assert_eq!((status, error(&body)), (400, "Unknown field: all"));
    let (status, _) = server.send("GET", RESET, Some(&header), None, "");
    assert_eq!(status, 405);
    assert!(!lock(&server.home).setup_required());
    // An empty body is `{}`.
    let (status, _) = server.send("POST", RESET, Some(&header), Some("application/json"), "");
    assert_eq!(status, 204);
}

// ---- The unguarded handler, for the existing tests ----

#[test]
fn the_unguarded_handler_serves_the_auth_routes_and_never_gates() {
    let mut home = Home::new(T0);
    home.set_presence(AlwaysConfirmed);
    let mut random = counter();
    let mut call = |home: &mut Home, method: &str, path: &str, body: &str| {
        handle(
            home,
            &Request {
                method,
                path,
                host: Some(HOST),
                port: PORT,
                reach: Reach::Loopback,
                content_type: Some("application/json"),
                authorization: None,
                content_length: None,
                body: body.as_bytes(),
            },
            T0,
            &mut *random,
        )
    };
    assert_eq!(call(&mut home, "GET", "/api/v1/zones", "").status, 200);
    let body = json!({"username": "admin", "password": PASSWORD, "displayName": "Owner"});
    assert_eq!(
        call(&mut home, "POST", "/api/v1/auth/setup", &body.to_string()).status,
        201
    );
    assert_eq!(call(&mut home, "GET", "/api/v1/zones", "").status, 200);
}

// ---- /ws's token ----

#[test]
fn the_token_of_a_ws_upgrade_is_read_from_the_header_or_the_second_subprotocol() {
    assert_eq!(
        bearer_token(["bearer.none, bearer.abc"], None).as_deref(),
        Some("abc")
    );
    assert_eq!(
        bearer_token(["bearer.none", "bearer.abc"], None).as_deref(),
        Some("abc")
    );
    assert_eq!(bearer_token(["bearer.abc"], None).as_deref(), Some("abc"));
    assert_eq!(bearer_token(["bearer.none"], None), None);
    assert_eq!(bearer_token(["other, x"], None), None);
    assert_eq!(bearer_token([], None), None);
    assert_eq!(bearer_token(["bearer."], None), None);
    // The header comes first.
    assert_eq!(
        bearer_token(["bearer.none, bearer.abc"], Some("Bearer x")).as_deref(),
        Some("x")
    );
    assert_eq!(bearer_token([], Some("Bearer x")).as_deref(), Some("x"));
    assert_eq!(bearer_token(["bearer.none"], Some("Bearer ")), None);
    assert_eq!(bearer_token([], Some("Basic x")), None);
}
