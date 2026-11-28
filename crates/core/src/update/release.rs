//! A release found and pulled by the box itself (spec 039, D65 to D69): the manifest of spec 038
//! and its rules (R1), where the box may fetch from (R3), the check and the pull (R2, R4), the
//! automatic check (R6). The core follows the redirects, applies the rules, bounds sizes and
//! times and keeps the cache; an adapter (`Fetcher`) only moves the bytes of one GET.

use std::cell::RefCell;
use std::io::{self, ErrorKind, Read};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::install::{Expected, mismatch, refuse};
use super::{HEADER_BYTES, Refusal, SLOT_BYTES, UpdateAdapter, Updates, Version};
use crate::api::{Response, error, private_ipv4, version_response};
use crate::backup::Pacing;
use crate::devices::lock;
use crate::failure::{Failure, Outcome};
use crate::home::CLOCK_SET_AFTER_MS;

/// The manifest is read whole into memory, at most this long (R1).
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;
/// The longest URL of any kind (the setting, `image.url`, a `Location`), in characters.
pub const MAX_URL_CHARS: usize = 2048;
/// The redirects followed for one fetch.
pub const MAX_REDIRECTS: usize = 5;
/// The least time between two checks asked through the API or a pull (ms, monotonic).
pub const MIN_CHECK_INTERVAL_MS: u64 = 60_000;
/// A pull uses the cached manifest when its check is younger than this (ms).
pub const FRESH_MS: u64 = 600_000;
/// The first automatic check, after the start (ms, monotonic).
pub const AUTO_FIRST_AFTER_MS: u64 = 600_000;
/// The automatic checks, one after the other (ms, monotonic).
pub const AUTO_EVERY_MS: u64 = 86_400_000;
/// A busy or unset clock postpones the automatic check by this much (ms).
pub const AUTO_RETRY_MS: u64 = 60_000;
/// The most resumed requests of one pull (spec 040, D72).
pub const RESUME_ATTEMPTS: u32 = 3;
/// The wait before the first resumed request (ms); doubled before each next one (2, 4, 8 s).
pub const RESUME_WAIT_MS: u64 = 2000;
/// An ETag longer than this (characters) is not compared.
pub const MAX_ETAG_CHARS: usize = 256;
/// The download hands the CPU back every this many bytes.
pub const PAUSE_EVERY_BYTES: u64 = 64 * 1024;
/// The manifest the box reads when the setting is unset.
pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/mchacher/solite/releases/latest/download/manifest.json";

/// The text of the 503 of a clock not set yet.
pub const CLOCK_NOT_SET: &str = "The clock is not set yet: checking a release needs the date";

/// Bounds of the waits of one fetch: each read at most `idle_ms` (the adapter's), the whole at
/// most `total_ms` (checked between reads, on the monotonic clock).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub idle_ms: u64,
    pub total_ms: u64,
}

impl Limits {
    /// A check alone (R4).
    pub const CHECK: Limits = Limits {
        idle_ms: 15_000,
        total_ms: 30_000,
    };
    /// A download (R4).
    pub const DOWNLOAD: Limits = Limits {
        idle_ms: 30_000,
        total_ms: 600_000,
    };
}

/// What one GET gave.
pub enum Fetched {
    /// 200: the declared length if any, the `ETag` if any, and the body.
    Body {
        length: Option<u64>,
        etag: Option<String>,
        body: Box<dyn Read>,
    },
    /// 206, to a request that asked a range: its `Content-Range` and `Content-Length` and `ETag`
    /// headers if any, and the body.
    Partial {
        content_range: Option<String>,
        length: Option<u64>,
        etag: Option<String>,
        body: Box<dyn Read>,
    },
    /// 301, 302, 303, 307, 308: the `Location` header, if any.
    Redirect(Option<String>),
    /// Any other status.
    Status(u16),
}

/// One GET, behind the platform (the host: `http` by itself and `https` by `curl`; the box: ESP-IDF's
/// HTTP client with its certificate bundle). Redirects are not followed: the core does it.
pub trait Fetcher {
    /// One GET of `url` (already checked by the core), each read bounded by `idle_ms`. The
    /// body owns what it needs (the connection): the core may call `get` again for a redirect.
    /// Errors are a message (connection refused, TLS failure, timed out...).
    fn get(&mut self, url: &str, idle_ms: u64) -> Result<Fetched, String>;

    /// One GET of `url` asking `Range: bytes=<from>-` (spec 040 R6). The default is a plain
    /// `get`: an adapter without range support behaves as a server without it.
    fn get_range(&mut self, url: &str, from: u64, idle_ms: u64) -> Result<Fetched, String> {
        let _ = from;
        self.get(url, idle_ms)
    }
}

// ---- The URL rule (R3) ----

/// A URL that passed R3's rule.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Url {
    https: bool,
    host: String,
    /// `scheme://authority`, to resolve a `Location` that is a path.
    origin: String,
}

/// Parses the form (R3's first bullet), without the plain-http rule: `None` is `invalid URL`.
fn parse_url(text: &str) -> Option<Url> {
    if text.len() > MAX_URL_CHARS || !text.bytes().all(|b| (b'!'..=b'~').contains(&b)) {
        return None;
    }
    let (scheme, rest) = text.split_once("://")?;
    let https = if scheme.eq_ignore_ascii_case("https") {
        true
    } else if scheme.eq_ignore_ascii_case("http") {
        false
    } else {
        return None;
    };
    let end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..end];
    // After the authority: nothing, or a path starting with `/` (the `find` above).
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
    {
        return None;
    }
    if let Some(port) = port
        && (port.is_empty()
            || port.len() > 5
            || !port.bytes().all(|b| b.is_ascii_digit())
            || port.parse::<u16>().ok().is_none_or(|p| p == 0))
    {
        return None;
    }
    Some(Url {
        https,
        host: host.to_ascii_lowercase(),
        origin: format!("{}://{authority}", scheme.to_ascii_lowercase()),
    })
}

/// True for `localhost`, or an IPv4 literal of exactly four decimal octets (no leading zero, so
/// no octal) of `10/8`, `172.16/12`, `192.168/16` or `127/8`.
fn lan_host(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    let plain = host
        .split('.')
        .all(|part| part == "0" || !part.starts_with('0'));
    plain && private_ipv4(host)
}

/// R3's rule for one URL: `Err` is the reason (`invalid URL`, `plain http is refused for <host>`).
fn check(text: &str) -> Result<Url, String> {
    let url = parse_url(text).ok_or_else(|| "invalid URL".to_string())?;
    if !url.https && !lan_host(&url.host) {
        return Err(format!("plain http is refused for {}", url.host));
    }
    Ok(url)
}

/// True when `text` is a URL the box may fetch from (the setting `update.manifestUrl`).
pub fn url_allowed(text: &str) -> bool {
    check(text).is_ok()
}

// ---- The manifest (R1) ----

/// What a check keeps: the cache's one entry, bounded by the fields below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The monotonic time of the check (ms).
    pub at_ms: u64,
    pub latest: Version,
    pub min_version: Option<Version>,
    /// At most `MAX_URL_CHARS`.
    pub url: String,
    pub size: u64,
    pub sha256: [u8; 32],
    pub release_url: Option<String>,
}

fn invalid(field: &str) -> String {
    format!("Invalid manifest: {field}")
}

fn hex32(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        let high = char::from(bytes[2 * i]).to_digit(16)?;
        let low = char::from(bytes[2 * i + 1]).to_digit(16)?;
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}

/// The release page of a GitHub asset address, `None` for any other.
fn release_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://github.com/")?;
    let rest = rest.split(['?', '#']).next()?;
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [owner, repo, "releases", "download", tag, name]
            if ![owner, repo, tag, name].iter().any(|p| p.is_empty()) =>
        {
            Some(format!(
                "https://github.com/{owner}/{repo}/releases/tag/{tag}"
            ))
        }
        _ => None,
    }
}

/// Reads the manifest's text (R1). `at_ms` is the time of the check.
fn parse_manifest(body: &[u8], at_ms: u64) -> Result<Checked, String> {
    let value: Value = serde_json::from_slice(body).map_err(|_| invalid("format"))?;
    let object = value.as_object().ok_or_else(|| invalid("format"))?;
    match object.get("format").and_then(Value::as_u64) {
        Some(1) => {}
        Some(other) => return Err(format!("Unsupported manifest format {other}")),
        None => return Err(invalid("format")),
    }
    let latest = object
        .get("version")
        .and_then(Value::as_str)
        .and_then(Version::parse)
        .ok_or_else(|| invalid("version"))?;
    let min_version = match object.get("minVersion") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_str()
                .and_then(Version::parse)
                .ok_or_else(|| invalid("minVersion"))?,
        ),
    };
    let image = object.get("image").and_then(Value::as_object);
    let field = |name: &str| image.and_then(|image| image.get(name));
    let size = field("size")
        .and_then(Value::as_u64)
        .filter(|size| (HEADER_BYTES as u64 + 1..=HEADER_BYTES as u64 + SLOT_BYTES).contains(size))
        .ok_or_else(|| invalid("image.size"))?;
    let sha256 = field("sha256")
        .and_then(Value::as_str)
        .and_then(hex32)
        .ok_or_else(|| invalid("image.sha256"))?;
    let url = field("url")
        .and_then(Value::as_str)
        .filter(|url| check(url).is_ok())
        .ok_or_else(|| invalid("image.url"))?;
    Ok(Checked {
        at_ms,
        latest,
        min_version,
        release_url: release_url(url),
        url: url.to_string(),
        size,
        sha256,
    })
}

// ---- Fetching with redirects (R3) ----

/// The reason text of a read error.
fn io_reason(error: &io::Error) -> String {
    match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => "timed out".to_string(),
        _ => error.to_string(),
    }
}

/// The text of the total time's failure.
fn too_long(limits: Limits) -> String {
    format!("took longer than {} s", limits.total_ms / 1000)
}

/// The elapsed time is over the limit.
fn over(pacing: &mut dyn Pacing, started: u64, limits: Limits) -> bool {
    pacing.now_ms().saturating_sub(started) > limits.total_ms
}

/// What a fetch of the download's address gave: a 200 or a 206 with its body.
enum Got {
    Body {
        length: Option<u64>,
        etag: Option<String>,
        body: Box<dyn Read>,
    },
    Partial {
        content_range: Option<String>,
        length: Option<u64>,
        etag: Option<String>,
        body: Box<dyn Read>,
    },
}

/// Why a fetch did not give a body.
enum Stop {
    /// Fails the pull at once (the URL rule, the total time, any other status).
    Fatal(String),
    /// A resumed request's blip: an adapter's error or a 5xx; counts as one attempt.
    Retry(String),
    /// A 416: the file is not the one the first answer gave.
    Changed,
}

impl Stop {
    fn text(self) -> String {
        match self {
            Stop::Fatal(reason) | Stop::Retry(reason) => reason,
            Stop::Changed => CHANGED.to_string(),
        }
    }
}

/// The reason of a resume that finds another file.
const CHANGED: &str = "the file changed on the server";

/// Fetches `start`, following the redirects (R3); every request asks `Range: bytes=<from>-`
/// when `range` is given (spec 040 R6.2).
fn fetch_chain(
    fetcher: &mut dyn Fetcher,
    start: &str,
    range: Option<u64>,
    limits: Limits,
    pacing: &mut dyn Pacing,
    started: u64,
) -> Result<Got, Stop> {
    let blip = |reason: String| {
        if range.is_some() {
            Stop::Retry(reason)
        } else {
            Stop::Fatal(reason)
        }
    };
    let mut url = start.to_string();
    let mut current = check(&url).map_err(Stop::Fatal)?;
    let mut redirects = 0;
    loop {
        if over(pacing, started, limits) {
            return Err(Stop::Fatal(too_long(limits)));
        }
        let answer = match range {
            Some(from) => fetcher.get_range(&url, from, limits.idle_ms),
            None => fetcher.get(&url, limits.idle_ms),
        }
        .map_err(&blip)?;
        match answer {
            Fetched::Body { length, etag, body } => return Ok(Got::Body { length, etag, body }),
            Fetched::Partial {
                content_range,
                length,
                etag,
                body,
            } => {
                if range.is_none() {
                    return Err(Stop::Fatal(format!("{url}: 206")));
                }
                return Ok(Got::Partial {
                    content_range,
                    length,
                    etag,
                    body,
                });
            }
            Fetched::Status(416) if range.is_some() => return Err(Stop::Changed),
            Fetched::Status(status) if range.is_some() && status >= 500 => {
                return Err(Stop::Retry(format!("{url}: {status}")));
            }
            Fetched::Status(status) => return Err(Stop::Fatal(format!("{url}: {status}"))),
            Fetched::Redirect(location) => {
                redirects += 1;
                if redirects > MAX_REDIRECTS {
                    return Err(Stop::Fatal("too many redirects".to_string()));
                }
                let location =
                    location.ok_or_else(|| Stop::Fatal("redirect without Location".to_string()))?;
                let next = if location.starts_with('/') && !location.starts_with("//") {
                    let joined = format!("{}{location}", current.origin);
                    if joined.len() > MAX_URL_CHARS {
                        return Err(Stop::Fatal("invalid URL".to_string()));
                    }
                    joined
                } else {
                    location
                };
                let parsed =
                    parse_url(&next).ok_or_else(|| Stop::Fatal("invalid URL".to_string()))?;
                if current.https && !parsed.https {
                    return Err(Stop::Fatal(
                        "redirect from https to http refused".to_string(),
                    ));
                }
                current = check(&next).map_err(Stop::Fatal)?;
                url = next;
            }
        }
    }
}

/// A 200's declared length, `ETag` and body.
type FirstAnswer = (Option<u64>, Option<String>, Box<dyn Read>);

/// A first, unranged fetch: a 200's length, `ETag` and body.
fn fetch(
    fetcher: &mut dyn Fetcher,
    start: &str,
    limits: Limits,
    pacing: &mut dyn Pacing,
    started: u64,
) -> Result<FirstAnswer, String> {
    match fetch_chain(fetcher, start, None, limits, pacing, started).map_err(Stop::text)? {
        Got::Body { length, etag, body } => Ok((length, etag, body)),
        // Not reachable (an unranged 206 is a `Stop`): kept as a failure, never a panic.
        Got::Partial { .. } => Err(format!("{start}: 206")),
    }
}

/// Fetches and reads the manifest at `url` (R1, R3, bounds of `Limits::CHECK`).
fn fetch_manifest(
    fetcher: &mut dyn Fetcher,
    url: &str,
    limits: Limits,
    pacing: &mut dyn Pacing,
) -> Result<Checked, String> {
    let text = fetch_text(
        fetcher,
        url,
        limits,
        pacing,
        MAX_MANIFEST_BYTES,
        "Manifest too large",
    )?;
    parse_manifest(&text, pacing.now_ms())
}

/// Fetches the bytes at `url` (R3's rules, `limits`), at most `max` of them: past it, `too_large`
/// is the reason. Also reads the recipe catalogue (spec 042).
pub(crate) fn fetch_text(
    fetcher: &mut dyn Fetcher,
    url: &str,
    limits: Limits,
    pacing: &mut dyn Pacing,
    max: usize,
    too_large: &str,
) -> Result<Vec<u8>, String> {
    let started = pacing.now_ms();
    let (_, _, mut body) = fetch(fetcher, url, limits, pacing, started)?;
    // Bound: the limit plus the one byte that shows it is too long.
    let mut text: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if over(pacing, started, limits) {
            return Err(too_long(limits));
        }
        let count = loop {
            match body.read(&mut chunk) {
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                other => break other,
            }
        }
        .map_err(|e| io_reason(&e))?;
        if count == 0 {
            break;
        }
        text.extend_from_slice(&chunk[..count]);
        if text.len() > max {
            return Err(too_large.to_string());
        }
    }
    Ok(text)
}

/// Why `download_file` gave no file.
#[cfg_attr(not(feature = "script-recipes"), allow(dead_code))]
pub(crate) enum FileError {
    /// The network or the server failed (the reason).
    Failed(String),
    /// What came is not what the catalogue says (size or SHA-256).
    Differs(&'static str),
}

/// Downloads a file of a known `size` and SHA-256 whole into memory (spec 042 R3.2): the update's
/// download, resumed as D72 says, at most three times, its total time bounded by `limits`.
#[cfg_attr(not(feature = "script-recipes"), allow(dead_code))]
pub(crate) fn download_file(
    fetcher: &mut dyn Fetcher,
    url: &str,
    size: u64,
    sha256: [u8; 32],
    limits: Limits,
    pacing: &mut dyn Pacing,
) -> Result<Vec<u8>, FileError> {
    let started = pacing.now_ms();
    let (length, etag, body) =
        fetch(fetcher, url, limits, pacing, started).map_err(FileError::Failed)?;
    if length.is_some_and(|length| length != size) {
        return Err(FileError::Differs(
            "Download size differs from the catalogue",
        ));
    }
    let audit = Rc::new(RefCell::new(Audit {
        count: 0,
        hash: Sha256::new(),
        failure: None,
        changed: false,
    }));
    let mut reader = Download {
        body,
        audit: Rc::clone(&audit),
        size,
        limits,
        started,
        since_pause: 0,
        fetcher,
        url: url.to_string(),
        etag: etag.filter(|etag| etag.len() <= MAX_ETAG_CHARS),
        resumes: 0,
        pacing,
    };
    // Bound: the catalogue's `size` (at most 32 KB and a header), plus the one byte that shows a longer body.
    let mut data: Vec<u8> = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    let mut chunk = [0u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => data.extend_from_slice(&chunk[..count]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                let reason = audit
                    .borrow()
                    .failure
                    .clone()
                    .unwrap_or_else(|| io_reason(&e));
                return Err(FileError::Failed(reason));
            }
        }
    }
    let audit = audit.borrow();
    if audit.count != size {
        return Err(FileError::Differs(
            "Download size differs from the catalogue",
        ));
    }
    if audit.hash.clone().finalize().as_slice() != sha256 {
        return Err(FileError::Differs(
            "Download SHA-256 differs from the catalogue",
        ));
    }
    Ok(data)
}

// ---- The state: the cache, one job at a time ----

#[derive(Debug)]
struct Inner {
    /// One entry, at most about 2.2 KB.
    cache: Option<Checked>,
    /// The kind of job whose ticket is held (a check, a pull), if any.
    busy: Option<Kind>,
    /// The start of the last check asked through the API or of a pull's check (monotonic).
    last_api_check_ms: Option<u64>,
    /// The start of the last catalogue refresh (wall clock), spec 042 R2.4.
    last_catalogue_ms: Option<u64>,
    /// The automatic check is due from this monotonic time on.
    next_auto_ms: u64,
}

/// What the home knows of the releases: the cache and the one-job-at-a-time mark, shared with
/// the tickets through an `Arc`.
#[derive(Debug)]
pub struct Releases {
    inner: Mutex<Inner>,
}

impl Default for Releases {
    fn default() -> Self {
        Releases {
            inner: Mutex::new(Inner {
                cache: None,
                busy: None,
                last_api_check_ms: None,
                last_catalogue_ms: None,
                next_auto_ms: AUTO_FIRST_AFTER_MS,
            }),
        }
    }
}

/// The answer of `GET /system/version` from the cache: the latest version, whether it is
/// available to install directly, and its page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub latest: Option<String>,
    pub update_available: bool,
    pub release_url: Option<String>,
}

/// What a ticket is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `POST /system/version/check`.
    Check,
    /// `POST /system/update` with an empty body.
    Pull,
    /// The automatic check.
    Auto,
    /// An install, an update or an uninstall of a script recipe (spec 042).
    Recipe,
    /// A read of the recipe catalogue (spec 042).
    Catalogue,
}

/// What a recipe job or a refresh is (spec 042 R3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeKind {
    Job,
    Refresh,
}

impl Releases {
    /// What `GET /system/version` says, from the cache and never from the network.
    pub fn info(&self, running: Version) -> Info {
        match &lock(&self.inner).cache {
            None => Info {
                latest: None,
                update_available: false,
                release_url: None,
            },
            Some(cached) => Info {
                latest: Some(cached.latest.to_string()),
                update_available: cached.latest > running
                    && cached.min_version.is_none_or(|min| min <= running),
                release_url: cached.release_url.clone(),
            },
        }
    }

    /// Drops the cached manifest: the next pull reads it again (a resume found another file).
    pub fn forget_cache(&self) {
        lock(&self.inner).cache = None;
    }

    /// The right to run one check or pull: 503 while the clock is not set (`wall_ms`), 409 while
    /// another job runs. The ticket ends the job when it is dropped.
    fn begin(
        self: &Arc<Self>,
        kind: Kind,
        manifest_url: &str,
        wall_ms: u64,
    ) -> Outcome<ReleaseTicket> {
        if wall_ms < CLOCK_SET_AFTER_MS {
            return Err(Failure::new(503, CLOCK_NOT_SET));
        }
        let mut inner = lock(&self.inner);
        if inner.busy.is_some() {
            return Err(Failure::new(
                409,
                if kind == Kind::Check {
                    "Update check already in progress"
                } else {
                    "Update already in progress"
                },
            ));
        }
        inner.busy = Some(kind);
        Ok(ReleaseTicket {
            releases: Arc::clone(self),
            manifest_url: manifest_url.to_string(),
        })
    }

    /// What a restart must not interrupt (spec 043): the text of the 409 while a check or a pull
    /// holds its ticket, else `None`.
    pub fn busy_message(&self) -> Option<&'static str> {
        lock(&self.inner).busy.map(message_of)
    }

    /// The text of the 409 of a backup, a restore or a factory reset while a recipe job (not a
    /// refresh) runs, else `None` (spec 042 R3.1).
    pub fn recipe_job_message(&self) -> Option<&'static str> {
        (lock(&self.inner).busy == Some(Kind::Recipe)).then_some(message_of(Kind::Recipe))
    }

    /// The right to run one recipe job or one refresh (spec 042 R3.1): 409 with the text of what
    /// runs while any job holds the session. Ends when the mark is dropped.
    pub fn begin_recipe(self: &Arc<Self>, kind: RecipeKind) -> Outcome<RecipeMark> {
        let mut inner = lock(&self.inner);
        if let Some(busy) = inner.busy {
            return Err(Failure::new(409, message_of(busy)));
        }
        inner.busy = Some(match kind {
            RecipeKind::Job => Kind::Recipe,
            RecipeKind::Refresh => Kind::Catalogue,
        });
        Ok(RecipeMark {
            releases: Arc::clone(self),
            status: None,
        })
    }

    /// 429 while a refresh was started less than `MIN_CHECK_INTERVAL_MS` ago (`now` is the wall
    /// clock); the start is not noted (`note_refresh`).
    pub fn pace_refresh(&self, now: u64) -> Result<(), Refusal> {
        let inner = lock(&self.inner);
        if let Some(last) = inner.last_catalogue_ms {
            let next = last.saturating_add(MIN_CHECK_INTERVAL_MS);
            if now < next {
                let seconds = (next - now).div_ceil(1000);
                return Err(Refusal::new(
                    429,
                    format!("Please wait {seconds}s before refreshing again"),
                ));
            }
        }
        Ok(())
    }

    /// A refresh starts at `now` (wall clock).
    pub fn note_refresh(&self, now: u64) {
        lock(&self.inner).last_catalogue_ms = Some(now);
    }

    /// For `POST /system/version/check` (R2).
    pub fn begin_check(
        self: &Arc<Self>,
        manifest_url: &str,
        wall_ms: u64,
    ) -> Outcome<ReleaseTicket> {
        self.begin(Kind::Check, manifest_url, wall_ms)
    }

    /// For `POST /system/update` with an empty body (R4).
    pub fn begin_pull(
        self: &Arc<Self>,
        manifest_url: &str,
        wall_ms: u64,
    ) -> Outcome<ReleaseTicket> {
        self.begin(Kind::Pull, manifest_url, wall_ms)
    }

    /// The automatic check (R6): `Some` when it is on (`enabled`), due on the monotonic clock
    /// (`mono_ms`), the clock set and no job running; then the next one is set 24 hours on. A busy
    /// or unset clock postpones it to the next minute.
    pub fn auto_ticket(
        self: &Arc<Self>,
        enabled: bool,
        manifest_url: &str,
        wall_ms: u64,
        mono_ms: u64,
    ) -> Option<ReleaseTicket> {
        if !enabled {
            return None;
        }
        {
            let inner = lock(&self.inner);
            if mono_ms < inner.next_auto_ms {
                return None;
            }
        }
        // The next check is set 24 hours on only once the ticket is taken: a refusal (busy, clock
        // not set) asks again in a minute.
        let ticket = self.begin(Kind::Auto, manifest_url, wall_ms).ok();
        lock(&self.inner).next_auto_ms = mono_ms.saturating_add(if ticket.is_some() {
            AUTO_EVERY_MS
        } else {
            AUTO_RETRY_MS
        });
        ticket
    }

    /// The automatic check's ticket was taken but its job could not start (no thread): the next
    /// ask is a minute after `mono_ms`, not 24 hours.
    pub fn postpone_auto(&self, mono_ms: u64) {
        lock(&self.inner).next_auto_ms = mono_ms.saturating_add(AUTO_RETRY_MS);
    }
}

/// The text of the 409 for what holds the session.
fn message_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Pull => "Update already in progress",
        Kind::Check | Kind::Auto => "Update check already in progress",
        Kind::Recipe => "Recipe install already in progress",
        Kind::Catalogue => "Recipe catalogue refresh already in progress",
    }
}

/// The right to run one recipe job or one refresh (spec 042). Dropping it ends the job, and the
/// recipe flag of the update status with it.
#[derive(Debug)]
pub struct RecipeMark {
    releases: Arc<Releases>,
    status: Option<Arc<crate::update::UpdateStatus>>,
}

impl RecipeMark {
    /// Sets the update status's recipe flag, ended with the mark.
    pub fn with_status(mut self, status: Arc<crate::update::UpdateStatus>) -> Self {
        self.status = Some(status);
        self
    }
}

impl Drop for RecipeMark {
    fn drop(&mut self) {
        if let Some(status) = &self.status {
            status.end_recipe_job();
        }
        lock(&self.releases.inner).busy = None;
    }
}

/// The right to run one check or pull. Dropping it ends the job.
#[derive(Debug)]
pub struct ReleaseTicket {
    releases: Arc<Releases>,
    manifest_url: String,
}

impl Drop for ReleaseTicket {
    fn drop(&mut self) {
        lock(&self.releases.inner).busy = None;
    }
}

/// The running version of this core.
fn running_version() -> Version {
    Version::parse(crate::VERSION).unwrap_or(Version::new(0, 0, 0))
}

impl ReleaseTicket {
    /// 429 while a check or a pull's check was started less than `MIN_CHECK_INTERVAL_MS` ago;
    /// else the start is noted.
    fn pace(&self, now: u64) -> Result<(), Refusal> {
        let mut inner = lock(&self.releases.inner);
        if let Some(last) = inner.last_api_check_ms {
            let next = last.saturating_add(MIN_CHECK_INTERVAL_MS);
            if now < next {
                let seconds = (next - now).div_ceil(1000);
                return Err(Refusal::new(
                    429,
                    format!("Please wait {seconds}s before checking again"),
                ));
            }
        }
        inner.last_api_check_ms = Some(now);
        Ok(())
    }

    /// Fetches the manifest and fills the cache; the reason text on a failure (logged).
    fn refresh(
        &self,
        fetcher: &mut dyn Fetcher,
        limits: Limits,
        pacing: &mut dyn Pacing,
    ) -> Result<Checked, String> {
        match fetch_manifest(fetcher, &self.manifest_url, limits, pacing) {
            Ok(checked) => {
                log::info!(
                    "update: check: latest {}, running {} ({})",
                    checked.latest,
                    running_version(),
                    self.manifest_url
                );
                lock(&self.releases.inner).cache = Some(checked.clone());
                Ok(checked)
            }
            Err(reason) => {
                log::warn!("update: check failed: {reason}");
                Err(reason)
            }
        }
    }

    /// `POST /system/version/check` (R2): 429 within 60 s of the last check, 500 `Update check
    /// failed: <reason>`, else 200 with the `VersionInfo`.
    pub fn check(
        self,
        fetcher: &mut dyn Fetcher,
        pacing: &mut dyn Pacing,
        limits: Limits,
    ) -> Response {
        if let Err(refusal) = self.pace(pacing.now_ms()) {
            return refuse(&refusal);
        }
        match self.refresh(fetcher, limits, pacing) {
            Ok(_) => version_response(&self.releases),
            Err(reason) => error(500, &format!("Update check failed: {reason}")),
        }
    }

    /// The automatic check (R6): fills the cache like `check`, never installs, never counts for
    /// the 60 s of the API; a failure is logged.
    pub fn check_auto(self, fetcher: &mut dyn Fetcher, pacing: &mut dyn Pacing, limits: Limits) {
        let _ = self.refresh(fetcher, limits, pacing);
    }

    /// `POST /system/update` with an empty body (R4): the manifest (cached when younger than
    /// `FRESH_MS`), its rules, the download through `Updates`' installation.
    pub fn pull(
        self,
        updates: &Updates,
        fetcher: &mut dyn Fetcher,
        adapter: &mut dyn UpdateAdapter,
        pacing: &mut dyn Pacing,
        check_limits: Limits,
        download_limits: Limits,
    ) -> Response {
        let guard = match updates.claim() {
            Ok(guard) => guard,
            Err(refusal) => return refuse(&refusal),
        };
        if adapter.pending() {
            return refuse(&Refusal::new(409, "The running image is not confirmed yet"));
        }
        let now = pacing.now_ms();
        let cached = lock(&self.releases.inner)
            .cache
            .clone()
            .filter(|cached| now.saturating_sub(cached.at_ms) < FRESH_MS);
        let manifest = match cached {
            Some(cached) => cached,
            None => {
                if let Err(refusal) = self.pace(now) {
                    return refuse(&refusal);
                }
                match self.refresh(fetcher, check_limits, pacing) {
                    Ok(checked) => checked,
                    Err(reason) => return error(500, &format!("Update check failed: {reason}")),
                }
            }
        };
        let running = updates.running();
        if manifest.latest <= running {
            return error(400, "No update available");
        }
        if let Some(min) = manifest.min_version
            && min > running
        {
            return error(
                400,
                &format!(
                    "Version {} needs {min} or later first (running {running}): install {min} before it",
                    manifest.latest
                ),
            );
        }
        if manifest.size > HEADER_BYTES as u64 + adapter.capacity() {
            return error(413, "Image too large for the update slot");
        }
        let started = pacing.now_ms();
        let (length, etag, body) =
            match fetch(fetcher, &manifest.url, download_limits, pacing, started) {
                Ok(fetched) => fetched,
                Err(reason) => return download_failed(&reason),
            };
        if length.is_some_and(|length| length != manifest.size) {
            return error(400, "Download size differs from the manifest");
        }
        let audit = Rc::new(RefCell::new(Audit {
            count: 0,
            hash: Sha256::new(),
            failure: None,
            changed: false,
        }));
        let mut reader = Download {
            body,
            audit: Rc::clone(&audit),
            size: manifest.size,
            limits: download_limits,
            started,
            since_pause: 0,
            fetcher,
            url: manifest.url.clone(),
            // Bound: an ETag over `MAX_ETAG_CHARS` is not kept (nor compared).
            etag: etag.filter(|etag| etag.len() <= MAX_ETAG_CHARS),
            resumes: 0,
            pacing,
        };
        let verify_audit = Rc::clone(&audit);
        let (size, sha256) = (manifest.size, manifest.sha256);
        let mut verify = move || {
            let audit = verify_audit.borrow();
            if audit.count == size && audit.hash.clone().finalize().as_slice() == sha256 {
                Ok(())
            } else {
                Err(mismatch())
            }
        };
        let response = updates.install_claimed(
            guard,
            &mut reader,
            adapter,
            Some(Expected {
                size: manifest.size,
                version: manifest.latest,
                verify: &mut verify,
            }),
        );
        if audit.borrow().changed {
            // The file is not the one the manifest named: the next pull reads the manifest again.
            self.releases.forget_cache();
        }
        let failure = audit.borrow().failure.clone();
        match failure {
            Some(reason) if response.status != 200 => download_failed(&reason),
            _ => response,
        }
    }
}

/// 500 `Download failed: <reason>`, logged.
fn download_failed(reason: &str) -> Response {
    log::warn!("update: download failed: {reason}");
    error(500, &format!("Download failed: {reason}"))
}

/// What the download's reader counts and keeps, shared with the installation's last check.
struct Audit {
    count: u64,
    hash: Sha256,
    /// The first failure the reader met: kept as the reason of the answer.
    failure: Option<String>,
    /// A resume found another file than the first answer's (R6.5): the cache is dropped.
    changed: bool,
}

/// The body, bounded to `size + 1` bytes, hashed and counted; a cut before `size` is resumed
/// with a `Range` request (R6), a read error past the attempts and the total time are failures
/// (R4).
struct Download<'a> {
    body: Box<dyn Read>,
    audit: Rc<RefCell<Audit>>,
    size: u64,
    limits: Limits,
    started: u64,
    since_pause: u64,
    fetcher: &'a mut dyn Fetcher,
    /// `image.url`, fetched again from the start of its redirects at each resume.
    url: String,
    /// The first answer's `ETag`.
    etag: Option<String>,
    /// The resumed requests made so far, at most `RESUME_ATTEMPTS`.
    resumes: u32,
    pacing: &'a mut dyn Pacing,
}

/// True when a 206's headers are the rest of the file asked for (R6.3).
fn partial_matches(
    size: u64,
    count: u64,
    first: Option<&str>,
    content_range: Option<&str>,
    length: Option<u64>,
    etag: Option<&str>,
) -> bool {
    let expected = format!("bytes {count}-{}/{size}", size.saturating_sub(1));
    if content_range.map(str::trim) != Some(expected.as_str()) {
        return false;
    }
    if length.is_some_and(|length| length != size - count) {
        return false;
    }
    match (first, etag) {
        (Some(a), Some(b)) if a.len() <= MAX_ETAG_CHARS && b.len() <= MAX_ETAG_CHARS => a == b,
        _ => true,
    }
}

impl Download<'_> {
    /// The body of a resumed request is the rest of the file, or the pull fails (R6.2 to R6.5):
    /// the cut body is dropped, the wait made, the address fetched again with the range.
    fn resume(&mut self, mut reason: String) -> io::Result<()> {
        loop {
            if self.resumes >= RESUME_ATTEMPTS {
                return Err(self.fail(format!("{reason} (after {RESUME_ATTEMPTS} resumes)")));
            }
            self.resumes += 1;
            // Closes the cut connection before any new one opens.
            self.body = Box::new(io::empty());
            let count = self.audit.borrow().count;
            log::info!(
                "update: download cut at {count} of {} bytes ({reason}), resuming ({}/{RESUME_ATTEMPTS})",
                self.size,
                self.resumes
            );
            self.pacing.wait_ms(RESUME_WAIT_MS << (self.resumes - 1));
            let got = fetch_chain(
                &mut *self.fetcher,
                &self.url,
                Some(count),
                self.limits,
                &mut *self.pacing,
                self.started,
            );
            match got {
                Ok(Got::Partial {
                    content_range,
                    length,
                    etag,
                    body,
                }) => {
                    if !partial_matches(
                        self.size,
                        count,
                        self.etag.as_deref(),
                        content_range.as_deref(),
                        length,
                        etag.as_deref(),
                    ) {
                        return Err(self.changed());
                    }
                    self.body = body;
                    return Ok(());
                }
                Ok(Got::Body { .. }) => {
                    return Err(self
                        .fail("the server does not resume (200 to a range request)".to_string()));
                }
                Err(Stop::Retry(next)) => reason = next,
                Err(Stop::Changed) => return Err(self.changed()),
                Err(Stop::Fatal(next)) => return Err(self.fail(next)),
            }
        }
    }

    fn changed(&self) -> io::Error {
        self.audit.borrow_mut().changed = true;
        self.fail(CHANGED.to_string())
    }

    fn fail(&self, reason: String) -> io::Error {
        let mut audit = self.audit.borrow_mut();
        if audit.failure.is_none() {
            audit.failure = Some(reason.clone());
        }
        io::Error::other(reason)
    }
}

impl Read for Download<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if let Some(reason) = self.audit.borrow().failure.clone() {
            return Err(io::Error::other(reason));
        }
        let count = self.audit.borrow().count;
        // One byte more than announced is read, to see a body that is too long.
        let room = self.size.saturating_add(1).saturating_sub(count);
        if room == 0 || out.is_empty() {
            return Ok(0);
        }
        if over(self.pacing, self.started, self.limits) {
            return Err(self.fail(too_long(self.limits)));
        }
        if self.since_pause >= PAUSE_EVERY_BYTES {
            self.since_pause = 0;
            self.pacing.pause();
        }
        let want = out.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        loop {
            let reason = match self.body.read(&mut out[..want]) {
                Ok(0) if count >= self.size => return Ok(0),
                Ok(0) => "connection lost".to_string(),
                Ok(read) => {
                    let mut audit = self.audit.borrow_mut();
                    audit.hash.update(&out[..read]);
                    audit.count += read as u64;
                    self.since_pause += read as u64;
                    return Ok(read);
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => return Err(e),
                // A failed read at the full count is the end of the body (R6.1).
                Err(_) if count >= self.size => return Ok(0),
                Err(e) => io_reason(&e),
            };
            self.resume(reason)?;
        }
    }
}
