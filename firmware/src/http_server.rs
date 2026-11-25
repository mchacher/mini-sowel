//! The radio image's HTTP server (feature `radio`, spec 023 R4): ESP-IDF's server on port 80, on
//! core 0. `/ws` is `live_ws.rs`'s; `/` and `/assets/*` are answered on the server's task from the
//! interface embedded in the flash, as the core's `static_reply` decides; every other request is
//! handed, as an ESP-IDF asynchronous request, to one of 4 worker threads (stacks and body buffers
//! in PSRAM), which asks the core's `handle_guarded` exactly as the Mac host does
//! (`crates/host/src/serve.rs`), so an order waiting for the radio never holds the server's task.
//! The core decides every answer; this file moves the bytes.

use std::ffi::{CStr, CString};
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::{MallocCap, ThreadSpawnConfiguration};
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::http::server::{Configuration, EspHttpServer, KeepAlive};
use esp_idf_svc::sys::{
    ESP_FAIL, ESP_OK, HTTPD_SOCK_ERR_TIMEOUT, MALLOC_CAP_8BIT, MALLOC_CAP_SPIRAM, esp_err_t,
    heap_caps_malloc, http_method_HTTP_DELETE, http_method_HTTP_GET, http_method_HTTP_HEAD,
    http_method_HTTP_PATCH, http_method_HTTP_POST, http_method_HTTP_PUT, httpd_handle_t,
    httpd_register_uri_handler, httpd_req_async_handler_begin, httpd_req_async_handler_complete,
    httpd_req_get_hdr_value_len, httpd_req_get_hdr_value_str, httpd_req_recv, httpd_req_t,
    httpd_req_to_sockfd, httpd_resp_send, httpd_resp_send_chunk, httpd_resp_set_hdr,
    httpd_resp_set_status, httpd_resp_set_type, httpd_sess_trigger_close, httpd_uri_t,
    uxTaskGetStackHighWaterMark2,
};
use solite_core::api::static_files::{STATIC_HEADERS, cache_control, find};
use solite_core::api::{
    MAX_BODY_BYTES, MAX_WORKERS, Reach, Reply, Request, Response, SlotGuard, Slots, handle_guarded,
    is_update_request, order_response, static_reply,
};
use solite_core::devices::lock;
use solite_core::home::Home;
use solite_core::update::MAX_UPLOAD_BYTES;

use crate::live_ws;
use crate::ota;
use crate::radio_engine::{Shared, fill, wall_ms};
use crate::ui_files::UI_FILES;

/// Each worker's stack, in PSRAM (R9.4): the workers never write the flash.
pub const WORKER_STACK_BYTES: usize = 10 * 1024;
/// Each worker's body buffer, in PSRAM (R4.4): one byte more than a body may hold.
const BODY_BUFFER_BYTES: usize = MAX_BODY_BYTES + 1;
/// The server's task stack, in internal RAM.
const SERVER_STACK_BYTES: usize = 8 * 1024;
/// At most this many connections; the least recently used one is closed beyond (R4.2).
const MAX_OPEN_SOCKETS: usize = 12;
/// The longest header value read (the request head is at most 1024 bytes, R4.2).
const MAX_HEADER_BYTES: usize = 1024;
/// The content type of every JSON answer.
const JSON: &str = "application/json; charset=utf-8";
const BUSY: &str = r#"{"error":"Busy"}"#;

/// What the server's handlers and the workers share.
struct Api {
    home: Arc<Mutex<Home>>,
    name: &'static str,
    port: u16,
    slots: Slots,
    queue: SyncSender<Job>,
}

static API: OnceLock<Api> = OnceLock::new();

/// The smallest unused stack of each worker since it started, in bytes (`heap:` line, R9.2).
static WORKER_STACK_UNUSED: [AtomicUsize; MAX_WORKERS] =
    [const { AtomicUsize::new(usize::MAX) }; MAX_WORKERS];

/// The smallest high-water mark among the workers and their stack, once the server runs.
pub fn http_stack() -> Option<(usize, usize)> {
    let least = WORKER_STACK_UNUSED
        .iter()
        .map(|unused| unused.load(Ordering::Relaxed))
        .min()
        .unwrap_or(usize::MAX);
    (least != usize::MAX).then_some((least, WORKER_STACK_BYTES))
}

/// The running server; dropping it stops the server (it is kept for the whole run).
pub struct Server {
    _server: EspHttpServer<'static>,
}

/// One request of ESP-IDF's server: valid while the handler that got it runs, or, for an
/// asynchronous copy, until `complete`.
struct Raw(*mut httpd_req_t);

// SAFETY: an asynchronous copy of a request (`httpd_req_async_handler_begin`) belongs to no task:
// ESP-IDF lets any task answer it until `httpd_req_async_handler_complete`; it is used by one
// worker at a time (it moves with its job).
#[allow(unsafe_code)]
unsafe impl Send for Raw {}

/// What the server's task read of the request before handing it over.
struct Head {
    method: &'static str,
    uri: String,
    host: Option<String>,
    content_type: Option<String>,
    /// The `Authorization` header, whole (the token goes in it, spec 029).
    authorization: Option<String>,
    chunked: bool,
    length: usize,
}

/// A request handed to a worker, with its slot (freed when the job ends).
struct Job {
    raw: Raw,
    head: Head,
    _slot: SlotGuard,
}

impl Raw {
    #[allow(unsafe_code, non_upper_case_globals)]
    fn method(&self) -> &'static str {
        // SAFETY: the request is valid (see `Raw`); reads one field.
        let method = unsafe { (*self.0).method } as u32;
        match method {
            http_method_HTTP_GET => "GET",
            http_method_HTTP_POST => "POST",
            http_method_HTTP_PUT => "PUT",
            http_method_HTTP_DELETE => "DELETE",
            http_method_HTTP_PATCH => "PATCH",
            http_method_HTTP_HEAD => "HEAD",
            _ => "OTHER",
        }
    }

    #[allow(unsafe_code)]
    fn uri(&self) -> String {
        // SAFETY: the request is valid; its `uri` field is a NUL-terminated string that ESP-IDF
        // keeps for the request's life, copied here.
        unsafe { CStr::from_ptr((*self.0).uri.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    #[allow(unsafe_code)]
    fn content_length(&self) -> usize {
        // SAFETY: the request is valid; reads one field.
        unsafe { (*self.0).content_len }
    }

    /// A header's value, at most `MAX_HEADER_BYTES`; `None` when absent.
    #[allow(unsafe_code)]
    fn header(&self, name: &CStr) -> Option<String> {
        // SAFETY: the request is valid; `name` is NUL-terminated; ESP-IDF writes at most
        // `buffer.len()` bytes, NUL included, into `buffer`, which outlives the call.
        unsafe {
            let length = httpd_req_get_hdr_value_len(self.0, name.as_ptr());
            if length == 0 || length > MAX_HEADER_BYTES {
                return None;
            }
            let mut buffer = vec![0u8; length + 1];
            if httpd_req_get_hdr_value_str(
                self.0,
                name.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            ) != ESP_OK
            {
                return None;
            }
            buffer.truncate(length);
            Some(String::from_utf8_lossy(&buffer).into_owned())
        }
    }

    /// Reads body bytes: their number, or ESP-IDF's error (`HTTPD_SOCK_ERR_TIMEOUT` after 5 s
    /// without a byte).
    #[allow(unsafe_code)]
    fn receive(&self, buffer: &mut [u8]) -> i32 {
        // SAFETY: the request is valid; ESP-IDF writes at most `buffer.len()` bytes into it.
        unsafe { httpd_req_recv(self.0, buffer.as_mut_ptr().cast(), buffer.len()) }
    }

    /// Sets the status line and the headers; the strings are returned, to be kept alive until the
    /// answer is sent (ESP-IDF keeps their pointers).
    #[allow(unsafe_code)]
    fn set_head(
        &self,
        status: u16,
        content_type: &str,
        headers: &[(&str, &str)],
    ) -> Result<Vec<CString>, String> {
        let c = |text: &str| CString::new(text).map_err(|e| e.to_string());
        let mut kept = vec![
            c(&format!("{status} {}", reason(status)))?,
            c(content_type)?,
        ];
        // SAFETY: the request is valid; the strings are NUL-terminated and kept by the caller
        // until the answer is sent.
        unsafe {
            httpd_resp_set_status(self.0, kept[0].as_ptr());
            httpd_resp_set_type(self.0, kept[1].as_ptr());
        }
        for (name, value) in headers {
            let (name, value) = (c(name)?, c(value)?);
            // SAFETY: as above.
            unsafe { httpd_resp_set_hdr(self.0, name.as_ptr(), value.as_ptr()) };
            kept.push(name);
            kept.push(value);
        }
        Ok(kept)
    }

    /// Sends a whole answer, with its length.
    #[allow(unsafe_code)]
    fn send(&self, body: &[u8]) -> bool {
        // SAFETY: the request is valid; `body` is valid for `body.len()` bytes during the call.
        unsafe { httpd_resp_send(self.0, body.as_ptr().cast(), body.len() as isize) == ESP_OK }
    }

    /// Sends one chunk of a chunked answer; an empty one ends it.
    #[allow(unsafe_code)]
    fn send_chunk(&self, chunk: &[u8]) -> bool {
        // SAFETY: as `send`.
        unsafe {
            httpd_resp_send_chunk(self.0, chunk.as_ptr().cast(), chunk.len() as isize) == ESP_OK
        }
    }

    /// A whole JSON answer (none for 204).
    fn answer(&self, response: &Response, headers: &[(&str, &str)]) {
        let Ok(_kept) = self.set_head(response.status, JSON, headers) else {
            return;
        };
        let body = response.body.as_deref().unwrap_or("");
        let _ = self.send(body.as_bytes());
    }

    /// What a synchronous handler returns once it has answered without reading the body: with a
    /// body declared, `ESP_FAIL`, so that ESP-IDF closes the connection instead of reading and
    /// discarding up to `Content-Length` bytes on the server's task (`httpd_req_delete`), which a
    /// slow client could hold for as long as it likes.
    fn unread_end(&self) -> esp_err_t {
        if self.content_length() > 0 {
            ESP_FAIL
        } else {
            ESP_OK
        }
    }

    /// Asks the server to close the connection once this answer is out.
    #[allow(unsafe_code)]
    fn close_after(&self) {
        // SAFETY: the request is valid; ESP-IDF queues the close of the socket on its own task.
        unsafe {
            let fd = httpd_req_to_sockfd(self.0);
            httpd_sess_trigger_close((*self.0).handle, fd);
        }
    }

    /// Ends an asynchronous request: ESP-IDF frees the copy and reads the next request of the
    /// connection.
    #[allow(unsafe_code)]
    fn complete(self) {
        // SAFETY: `self` is an asynchronous copy, completed once (it is consumed here).
        unsafe { httpd_req_async_handler_complete(self.0) };
    }
}

/// The reason phrase of a status.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Status",
    }
}

fn error_body(status: u16, message: &str) -> Response {
    Response {
        status,
        body: Some(json_error(message)),
    }
}

/// `{"error":"<message>"}` for the firmware's own refusals (fixed ASCII messages).
fn json_error(message: &str) -> String {
    format!(r#"{{"error":"{message}"}}"#)
}

// ---- The interface, on the server's task ----

/// `/` and `/assets/*`, every method: the core's `static_reply` decides, the file comes from the
/// flash, gzip as built.
#[allow(unsafe_code)]
extern "C" fn static_entry(request: *mut httpd_req_t) -> esp_err_t {
    let Some(api) = API.get() else {
        return ESP_FAIL;
    };
    let raw = Raw(request);
    let uri = raw.uri();
    let host = raw.header(c"Host");
    let reply = static_reply(&Request {
        method: raw.method(),
        path: &uri,
        host: host.as_deref(),
        port: api.port,
        reach: Reach::Lan { name: api.name },
        content_type: None,
        authorization: None,
        body: b"",
    });
    match reply {
        Some(Reply::Static(file)) => match find(UI_FILES, &file.path) {
            Some(bytes) => {
                let mut headers = vec![
                    ("Content-Encoding", "gzip"),
                    ("Vary", "Accept-Encoding"),
                    ("Cache-Control", cache_control(&file)),
                ];
                headers.extend_from_slice(&STATIC_HEADERS);
                if let Ok(_kept) = raw.set_head(200, file.content_type, &headers) {
                    let _ = raw.send(bytes);
                }
            }
            None => raw.answer(&error_body(404, "Not found"), &STATIC_HEADERS),
        },
        Some(Reply::StaticError(response)) => raw.answer(&response, &STATIC_HEADERS),
        Some(Reply::Whole(response)) => raw.answer(&response, &[]),
        Some(Reply::List(_) | Reply::Order(_) | Reply::Update) | None => {
            raw.answer(&error_body(404, "Not found"), &STATIC_HEADERS);
        }
    }
    raw.unread_end()
}

// ---- The API: the server's task hands each request to a worker ----

/// Every other path: a slot (503 `Busy` beyond 4 in progress), the head read, then an
/// asynchronous copy for a worker.
#[allow(unsafe_code)]
extern "C" fn api_entry(request: *mut httpd_req_t) -> esp_err_t {
    let Some(api) = API.get() else {
        return ESP_FAIL;
    };
    let raw = Raw(request);
    let Some(slot) = api.slots.try_take() else {
        if let Ok(_kept) = raw.set_head(503, JSON, &[]) {
            let _ = raw.send(BUSY.as_bytes());
        }
        return raw.unread_end();
    };
    let head = Head {
        method: raw.method(),
        uri: raw.uri(),
        host: raw.header(c"Host"),
        content_type: raw.header(c"Content-Type"),
        authorization: raw.header(c"Authorization"),
        chunked: raw.header(c"Transfer-Encoding").is_some(),
        length: raw.content_length(),
    };
    let mut copy: *mut httpd_req_t = std::ptr::null_mut();
    // SAFETY: `request` is the live request of this handler; ESP-IDF writes the copy's address.
    if unsafe { httpd_req_async_handler_begin(request, &mut copy) } != ESP_OK || copy.is_null() {
        raw.answer(&error_body(500, "Internal error"), &[]);
        return raw.unread_end();
    }
    let job = Job {
        raw: Raw(copy),
        head,
        _slot: slot,
    };
    // The slots admit at most `MAX_WORKERS` jobs, the queue holds as many: never full.
    if let Err(TrySendError::Full(job) | TrySendError::Disconnected(job)) = api.queue.try_send(job)
    {
        job.raw.answer(&error_body(503, "Busy"), &[]);
        job.raw.complete();
    }
    ESP_OK
}

/// The update's body as a `Read`: the bytes of the request, ending at its `Content-Length` (so that
/// "one more byte than declared" never waits on the socket). A socket error or timeout is an
/// error, the installation then aborts.
struct BodyReader<'a> {
    raw: &'a Raw,
    remaining: usize,
}

impl Read for BodyReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let want = buffer.len().min(self.remaining);
        if want == 0 {
            return Ok(0);
        }
        let count = self.raw.receive(&mut buffer[..want]);
        if count < 0 {
            return Err(std::io::Error::other(format!("socket error {count}")));
        }
        let count = count as usize;
        self.remaining -= count;
        Ok(count)
    }
}

/// `POST /api/v1/system/update`, once the core has let it through: the body goes into
/// `Updates::install`, through the OTA thread. Answers; true when the update is installed and the
/// ESP32 must restart (after `complete`, so the answer is out first).
fn answer_update(raw: &Raw, length: usize) -> bool {
    let (Some(updates), Some(mut adapter)) = (ota::updates(), ota::adapter()) else {
        raw.answer(&error_body(503, "Updates are not available"), &[]);
        raw.close_after();
        return false;
    };
    let mut source = BodyReader {
        raw,
        remaining: length,
    };
    let response = updates.install(&mut source, &mut adapter);
    let installed = response.status == 200;
    raw.answer(&response, &[]);
    if !installed {
        raw.close_after();
    }
    installed
}

/// A worker's answer to one request, as the host's `answer` (`crates/host/src/serve.rs`).
fn answer(job: Job, buffer: &mut [u8], api: &Api) {
    let Job { raw, head, _slot } = job;
    // The update's body is a file of megabytes: it is not read here (spec 032 R6), the core answers
    // the gate first, and the body streams into the installation.
    let streaming = is_update_request(head.method, &head.uri);
    if head.chunked {
        raw.answer(&error_body(411, "Length required"), &[]);
        raw.close_after();
        return raw.complete();
    }
    if streaming && raw.header(c"Content-Length").is_none() {
        raw.answer(&error_body(411, "Length required"), &[]);
        raw.close_after();
        return raw.complete();
    }
    if !streaming && head.length > MAX_BODY_BYTES {
        raw.answer(&error_body(413, "Body too large"), &[]);
        raw.close_after();
        return raw.complete();
    }
    let mut received = 0;
    while !streaming && received < head.length {
        let count = raw.receive(&mut buffer[received..head.length]);
        if count <= 0 {
            if count == HTTPD_SOCK_ERR_TIMEOUT {
                raw.answer(&error_body(408, "Request timeout"), &[]);
            }
            raw.close_after();
            return raw.complete();
        }
        received += count as usize;
    }
    // The core takes and lets go of the home itself: a password hash runs without it.
    let reply = {
        let mut random = |bytes: &mut [u8]| fill(bytes);
        handle_guarded(
            &api.home,
            &Request {
                method: head.method,
                path: &head.uri,
                host: head.host.as_deref(),
                port: api.port,
                reach: Reach::Lan { name: api.name },
                content_type: head.content_type.as_deref(),
                authorization: head.authorization.as_deref(),
                body: &buffer[..received],
            },
            wall_ms(),
            &mut random,
        )
    };
    // An order waits for the radio loop's answer, without the lock.
    let reply = match reply {
        Reply::Order(wait) => Reply::Whole(order_response(wait)),
        other => other,
    };
    // The gate answered first; then a declared length above the maximum is refused (no body read).
    let too_large = matches!(reply, Reply::Update) && head.length as u64 > MAX_UPLOAD_BYTES;
    // Every refusal of the update route leaves body bytes unread: the connection closes after the
    // answer.
    let close_after_answer = streaming && (too_large || !matches!(reply, Reply::Update));
    let mut restart = false;
    if too_large {
        raw.answer(&error_body(413, "Update too large"), &[]);
        if close_after_answer {
            raw.close_after();
        }
        return raw.complete();
    }
    match reply {
        Reply::Whole(response) => raw.answer(&response, &[]),
        Reply::StaticError(response) => raw.answer(&response, &STATIC_HEADERS),
        Reply::List(mut cursor) => {
            if let Ok(_kept) = raw.set_head(200, JSON, &[]) {
                loop {
                    // The home is taken for one chunk, and let go while it is written (D15).
                    let chunk = cursor.next_chunk(&lock(&api.home));
                    match chunk {
                        Ok(Some(chunk)) => {
                            if !raw.send_chunk(chunk.as_bytes()) {
                                break;
                            }
                        }
                        Ok(None) => {
                            let _ = raw.send_chunk(&[]);
                            break;
                        }
                        // The client sees a broken stream, not a short list.
                        Err(_) => {
                            raw.close_after();
                            break;
                        }
                    }
                }
            }
        }
        Reply::Order(_) | Reply::Static(_) => raw.answer(&error_body(500, "Internal error"), &[]),
        Reply::Update => restart = answer_update(&raw, head.length),
    }
    if close_after_answer {
        raw.close_after();
    }
    raw.complete();
    if restart {
        ota::restart_after_update();
    }
}

/// A buffer of `BODY_BUFFER_BYTES` in PSRAM, kept for the server's life.
#[allow(unsafe_code)]
fn body_buffer() -> Result<&'static mut [u8], String> {
    // SAFETY: asks ESP-IDF for a block of PSRAM; checked for null below.
    let pointer =
        unsafe { heap_caps_malloc(BODY_BUFFER_BYTES, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT) };
    if pointer.is_null() {
        return Err("serving: no PSRAM for a body buffer".to_string());
    }
    // SAFETY: the block is `BODY_BUFFER_BYTES` long, never freed (kept for the run), and owned by
    // this one slice; it is zeroed before use.
    let buffer = unsafe { std::slice::from_raw_parts_mut(pointer.cast::<u8>(), BODY_BUFFER_BYTES) };
    buffer.fill(0);
    Ok(buffer)
}

/// The calling task's unused stack, in bytes.
#[allow(unsafe_code)]
fn stack_unused() -> usize {
    // SAFETY: a null handle means the calling task; only reads its high-water mark.
    unsafe { uxTaskGetStackHighWaterMark2(std::ptr::null_mut()) as usize }
}

/// Starts the 4 workers on core 0, their stacks in PSRAM, each with its own body buffer.
fn start_workers(jobs: Receiver<Job>) -> Result<(), String> {
    let jobs = Arc::new(Mutex::new(jobs));
    ThreadSpawnConfiguration {
        name: Some(c"http-worker"),
        stack_size: WORKER_STACK_BYTES,
        pin_to_core: Some(Core::Core0),
        stack_alloc_caps: MallocCap::Spiram | MallocCap::Cap8bit,
        ..Default::default()
    }
    .set()
    .map_err(|e| format!("serving: cannot configure the workers: {e}"))?;
    let mut result = Ok(());
    for (index, unused) in WORKER_STACK_UNUSED.iter().enumerate() {
        let buffer = match body_buffer() {
            Ok(buffer) => buffer,
            Err(e) => {
                result = Err(e);
                break;
            }
        };
        let jobs = Arc::clone(&jobs);
        let spawned = std::thread::Builder::new()
            .name(format!("http-worker-{index}"))
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || {
                unused.store(stack_unused(), Ordering::Relaxed);
                loop {
                    let job = match lock(&jobs).recv() {
                        Ok(job) => job,
                        Err(_) => return,
                    };
                    if let Some(api) = API.get() {
                        answer(job, buffer, api);
                    }
                    unused.fetch_min(stack_unused(), Ordering::Relaxed);
                }
            });
        if let Err(e) = spawned {
            result = Err(format!("serving: cannot start a worker: {e}"));
            break;
        }
    }
    // Threads started later get the default configuration again.
    let _ = ThreadSpawnConfiguration::default().set();
    result
}

/// Registers a handler of ours for a path and a method.
#[allow(unsafe_code)]
fn register(
    server: httpd_handle_t,
    uri: &'static CStr,
    method: u32,
    handler: extern "C" fn(*mut httpd_req_t) -> esp_err_t,
) -> Result<(), String> {
    let conf = httpd_uri_t {
        uri: uri.as_ptr(),
        method,
        handler: Some(handler),
        user_ctx: std::ptr::null_mut(),
        ..Default::default()
    };
    // SAFETY: the server handle is live; ESP-IDF copies the description (and the path) it is
    // given; the path is static anyway.
    let code = unsafe { httpd_register_uri_handler(server, &conf) };
    if code == ESP_OK {
        Ok(())
    } else {
        Err(format!(
            "cannot register {} (error {code})",
            uri.to_string_lossy()
        ))
    }
}

/// Starts the server for the shared home: `/ws`, the interface, then the API on every other
/// path (ESP-IDF tries the handlers in the order they were registered).
pub fn start(shared: Shared, name: &'static str, port: u16) -> Result<Server, String> {
    let (queue, jobs) = sync_channel(MAX_WORKERS);
    let api = Api {
        home: Arc::clone(&shared.home),
        name,
        port,
        slots: Slots::new(MAX_WORKERS),
        queue,
    };
    API.set(api)
        .map_err(|_| "the server is already started".to_string())?;
    start_workers(jobs)?;
    let configuration = Configuration {
        http_port: port,
        core: Some(Core::Core0),
        stack_size: SERVER_STACK_BYTES,
        max_open_sockets: MAX_OPEN_SOCKETS,
        lru_purge_enable: true,
        uri_match_wildcard: true,
        max_resp_headers: 10,
        keep_alive: Some(KeepAlive {
            idle_secs: 30,
            interval_secs: 5,
            probe_count: 3,
        }),
        ..Default::default()
    };
    let server = EspHttpServer::new(&configuration).map_err(|e| e.to_string())?;
    let handle = server.handle();
    live_ws::register(handle, shared.hub, Arc::clone(&shared.home), name, port)?;
    for method in [
        http_method_HTTP_GET,
        http_method_HTTP_POST,
        http_method_HTTP_PUT,
        http_method_HTTP_DELETE,
    ] {
        register(handle, c"/", method, static_entry)?;
        register(handle, c"/assets/*", method, static_entry)?;
    }
    for method in [
        http_method_HTTP_GET,
        http_method_HTTP_POST,
        http_method_HTTP_PUT,
        http_method_HTTP_DELETE,
        http_method_HTTP_PATCH,
    ] {
        register(handle, c"/*", method, api_entry)?;
    }
    ota::API_LISTENING.store(true, Ordering::SeqCst);
    Ok(Server { _server: server })
}
