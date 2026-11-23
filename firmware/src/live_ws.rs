//! The radio image's live events (feature `radio`, spec 023 R5): `/ws` as an ESP-IDF WebSocket
//! handler registered directly, so that ESP-IDF answers the handshake and then calls
//! `after_handshake` on that same request (`CONFIG_HTTPD_WS_POST_HANDSHAKE_CB_SUPPORT`): the
//! `Host`, the `Origin` and a slot of the core's `LiveHub` are checked there, a refusal being a
//! close code (4003, 4003, 1013), and the welcome is sent. ESP-IDF 5.5 never calls a WebSocket
//! URI handler for the handshake itself, only for the frames that follow, so esp-idf-svc's
//! `ws_handler` (whose `New` case is the handshake) never sees it. Each session holds a context
//! whose release, when ESP-IDF deletes the socket, frees its slot. A live thread on core 0 sends
//! each client its batch every 200 ms as one text message in fragments, and a ping every 30 s,
//! through work queued on the server's task. The rules are the core's (`live::session`,
//! `LiveHub`), shared with the host's `ws.rs`.

use std::ffi::{CStr, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::sys::{
    ESP_FAIL, ESP_OK, SHUT_RDWR, esp_err_t, http_method_HTTP_GET, httpd_handle_t, httpd_queue_work,
    httpd_register_uri_handler, httpd_req_get_hdr_value_len, httpd_req_get_hdr_value_str,
    httpd_req_t, httpd_req_to_sockfd, httpd_sess_trigger_close, httpd_uri_t, httpd_ws_frame_t,
    httpd_ws_recv_frame, httpd_ws_send_frame, httpd_ws_send_frame_async,
    httpd_ws_type_t_HTTPD_WS_TYPE_BINARY, httpd_ws_type_t_HTTPD_WS_TYPE_CLOSE,
    httpd_ws_type_t_HTTPD_WS_TYPE_CONTINUE, httpd_ws_type_t_HTTPD_WS_TYPE_PING,
    httpd_ws_type_t_HTTPD_WS_TYPE_TEXT, lwip_shutdown,
};
use solite_core::api::{Reach, host_allowed};
use solite_core::devices::lock;
use solite_core::live::session::{
    MAX_INCOMING_BYTES, PING_SECONDS, TICK_MS, close_reason, origin_allowed,
};
use solite_core::live::{ClientId, LiveHandle, MAX_CLIENTS};

/// The live thread's stack, in internal RAM.
const LIVE_STACK_BYTES: usize = 4 * 1024;
/// The longest header read during the handshake (the head is at most 1024 bytes, R4.2).
const MAX_HEADER_BYTES: usize = 1024;
/// The subprotocol the interface offers (`ui/src/store/useLive.ts`), the only one echoed.
const SUBPROTOCOL: &CStr = c"bearer.none";
/// The first message, as the host sends it (spec 013, `crates/host/src/ws.rs`).
const WELCOME: &str =
    r#"{"type":"connected","message":"Connected to Sowel engine","version":"0.1.0"}"#;

/// What the handlers and the live thread share.
struct Live {
    hub: LiveHandle,
    name: &'static str,
    port: u16,
    server: httpd_handle_t,
}

// SAFETY: the server handle is an opaque pointer ESP-IDF lets any task use for its queued calls
// (`httpd_sess_trigger_close`, `httpd_queue_work`).
#[allow(unsafe_code)]
unsafe impl Send for Live {}
// SAFETY: as above; the hub is itself `Sync`.
#[allow(unsafe_code)]
unsafe impl Sync for Live {}

static LIVE: OnceLock<Live> = OnceLock::new();

/// One live client: its socket, its id in the hub, whether ESP-IDF has deleted its socket, the
/// message being received.
struct Session {
    fd: i32,
    client: ClientId,
    closed: Arc<AtomicBool>,
    incoming: Vec<u8>,
}

/// The sessions, at most `MAX_CLIENTS` (the hub refuses a 5th).
static SESSIONS: Mutex<Vec<Session>> = Mutex::new(Vec::new());

/// The frame kinds the live events send.
#[derive(Clone, Copy)]
enum Frame {
    /// A text frame; `true` when more fragments follow.
    Text(bool),
    /// A continuation; `true` for the last fragment.
    Continue(bool),
    Ping,
    Close,
}

/// ESP-IDF's description of a frame to send (it only reads `payload`).
fn raw_frame(kind: Frame, payload: &[u8]) -> httpd_ws_frame_t {
    let (type_, final_, fragmented) = match kind {
        Frame::Text(more) => (httpd_ws_type_t_HTTPD_WS_TYPE_TEXT, !more, more),
        Frame::Continue(last) => (httpd_ws_type_t_HTTPD_WS_TYPE_CONTINUE, last, true),
        Frame::Ping => (httpd_ws_type_t_HTTPD_WS_TYPE_PING, true, false),
        Frame::Close => (httpd_ws_type_t_HTTPD_WS_TYPE_CLOSE, true, false),
    };
    httpd_ws_frame_t {
        final_,
        fragmented,
        type_,
        payload: payload.as_ptr().cast_mut(),
        len: payload.len(),
    }
}

/// `Host` and `Origin` of the handshake, copied while the request is valid.
#[allow(unsafe_code)]
fn handshake_headers(request: *mut httpd_req_t) -> (Option<String>, Option<String>) {
    let read = |name: &CStr| -> Option<String> {
        // SAFETY: `request` is the handshake's request, valid during `after_handshake`; `name` is
        // NUL-terminated; ESP-IDF writes at most `buffer.len()` bytes, NUL included, into
        // `buffer`, which outlives the call. Both values are copied out.
        unsafe {
            let length = httpd_req_get_hdr_value_len(request, name.as_ptr());
            if length == 0 || length > MAX_HEADER_BYTES {
                return None;
            }
            let mut buffer = vec![0u8; length + 1];
            if httpd_req_get_hdr_value_str(
                request,
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
    };
    (read(c"Host"), read(c"Origin"))
}

/// The payload of a close frame: the code, then its reason.
fn close_payload(code: u16, reason: &str) -> Vec<u8> {
    let mut payload = code.to_be_bytes().to_vec();
    payload.extend_from_slice(reason.as_bytes());
    payload
}

/// Asks the server to close a socket (queued on its task).
#[allow(unsafe_code)]
fn trigger_close(server: httpd_handle_t, fd: i32) {
    // SAFETY: the server handle is live for the whole run; ESP-IDF only queues the close of `fd`.
    unsafe { httpd_sess_trigger_close(server, fd) };
}

/// Sends a frame on the request being handled (the server's task only).
#[allow(unsafe_code)]
fn send_on(request: *mut httpd_req_t, kind: Frame, payload: &[u8]) -> esp_err_t {
    let mut frame = raw_frame(kind, payload);
    // SAFETY: `request` is the WebSocket request being handled on the server's task; ESP-IDF
    // reads `payload` during the call only.
    unsafe { httpd_ws_send_frame(request, &mut frame) }
}

/// Sends a close frame on the request's connection and closes its socket.
fn refuse(request: *mut httpd_req_t, server: httpd_handle_t, fd: i32, code: u16, reason: &str) {
    let _ = send_on(request, Frame::Close, &close_payload(code, reason));
    trigger_close(server, fd);
}

/// The socket of a request.
#[allow(unsafe_code)]
fn socket_of(request: *mut httpd_req_t) -> i32 {
    // SAFETY: `request` is the request being handled.
    unsafe { httpd_req_to_sockfd(request) }
}

/// Releases a session's context: ESP-IDF calls it on its task when it deletes the socket. The
/// session is marked closed (no more sends) and its slot freed.
#[allow(unsafe_code)]
extern "C" fn session_closed(context: *mut c_void) {
    // SAFETY: `context` is the `Arc::into_raw` given in `after_handshake`, released once here.
    let closed = unsafe { Arc::from_raw(context.cast_const().cast::<AtomicBool>()) };
    closed.store(true, Ordering::SeqCst);
    let mut sessions = lock(&SESSIONS);
    if let Some(at) = sessions
        .iter()
        .position(|s| Arc::ptr_eq(&s.closed, &closed))
    {
        let session = sessions.swap_remove(at);
        if let Some(live) = LIVE.get() {
            live.hub.disconnect(session.client);
        }
    }
}

/// Called by ESP-IDF on the handshake's request right after its 101: checks, slot, welcome.
#[allow(unsafe_code)]
extern "C" fn after_handshake(request: *mut httpd_req_t) -> esp_err_t {
    let Some(live) = LIVE.get() else {
        return ESP_OK;
    };
    let fd = socket_of(request);
    let (host, origin) = handshake_headers(request);
    if !host_allowed(host.as_deref(), live.port, Reach::Lan { name: live.name }) {
        refuse(request, live.server, fd, 4003, "Forbidden host");
        return ESP_OK;
    }
    if !origin_allowed(origin.as_deref(), host.as_deref()) {
        refuse(request, live.server, fd, 4003, "Origin not allowed");
        return ESP_OK;
    }
    let Some(client) = live.hub.connect() else {
        refuse(request, live.server, fd, 1013, close_reason(1013));
        return ESP_OK;
    };
    let closed = Arc::new(AtomicBool::new(false));
    lock(&SESSIONS).push(Session {
        fd,
        client,
        closed: Arc::clone(&closed),
        incoming: Vec::new(),
    });
    // SAFETY: the request is the one being handled; ESP-IDF moves its session context to the
    // socket when the request ends and releases it with `session_closed` when the socket is
    // deleted (`httpd_sess_clear_ctx`).
    unsafe {
        (*request).sess_ctx = Arc::into_raw(closed).cast_mut().cast();
        (*request).free_ctx = Some(session_closed);
    }
    send_on(request, Frame::Text(false), WELCOME.as_bytes())
}

/// One data frame (ESP-IDF answers the control frames itself): kept until its message ends, then
/// a subscribe; past `MAX_INCOMING_BYTES`, the client is closed with 1009.
#[allow(unsafe_code)]
extern "C" fn on_frame(request: *mut httpd_req_t) -> esp_err_t {
    let Some(live) = LIVE.get() else {
        return ESP_OK;
    };
    let fd = socket_of(request);
    let mut frame = raw_frame(Frame::Close, &[]);
    // SAFETY: the frame's request, on the server's task; a zero length reads only its header.
    if unsafe { httpd_ws_recv_frame(request, &mut frame, 0) } != ESP_OK {
        return ESP_FAIL;
    }
    if frame.len > MAX_INCOMING_BYTES {
        too_big(request, live, fd);
        return ESP_OK;
    }
    let mut buffer = [0u8; MAX_INCOMING_BYTES];
    if frame.len > 0 {
        frame.payload = buffer.as_mut_ptr();
        // SAFETY: as above; ESP-IDF writes at most `frame.len` bytes, which fit in `buffer`.
        if unsafe { httpd_ws_recv_frame(request, &mut frame, frame.len) } != ESP_OK {
            return ESP_FAIL;
        }
    }
    // ESP-IDF gives a received frame's FIN bit in `final_`.
    #[allow(non_upper_case_globals)]
    match frame.type_ {
        httpd_ws_type_t_HTTPD_WS_TYPE_TEXT
        | httpd_ws_type_t_HTTPD_WS_TYPE_BINARY
        | httpd_ws_type_t_HTTPD_WS_TYPE_CONTINUE => {}
        // A pong, which ESP-IDF hands over too.
        _ => return ESP_OK,
    }
    let ends = frame.final_;
    let data = &buffer[..frame.len];
    let message = {
        let mut sessions = lock(&SESSIONS);
        let Some(session) = sessions.iter_mut().find(|s| s.fd == fd) else {
            return ESP_OK;
        };
        if session.incoming.len() + data.len() > MAX_INCOMING_BYTES {
            session.incoming.clear();
            drop(sessions);
            too_big(request, live, fd);
            return ESP_OK;
        }
        session.incoming.extend_from_slice(data);
        if !ends {
            return ESP_OK;
        }
        (session.client, std::mem::take(&mut session.incoming))
    };
    let (client, bytes) = message;
    live.hub.subscribe(client, &String::from_utf8_lossy(&bytes));
    ESP_OK
}

fn too_big(request: *mut httpd_req_t, live: &Live, fd: i32) {
    refuse(request, live.server, fd, 1009, close_reason(1009));
}

/// A frame for the server's task to send, and where its result goes.
struct Queued {
    server: httpd_handle_t,
    fd: i32,
    closed: Arc<AtomicBool>,
    frame: httpd_ws_frame_t,
    done: SyncSender<esp_err_t>,
}

/// Runs on the server's task: sends the frame unless the session was closed meanwhile (its socket
/// number may already belong to another connection).
#[allow(unsafe_code)]
extern "C" fn send_queued(argument: *mut c_void) {
    // SAFETY: `argument` is the `Box<Queued>` leaked by `send`, taken back once here.
    let mut queued = unsafe { Box::from_raw(argument.cast::<Queued>()) };
    let result = if queued.closed.load(Ordering::SeqCst) {
        ESP_FAIL
    } else {
        // SAFETY: on the server's task, for a socket still open; the payload outlives the call
        // (`send` waits for this result).
        unsafe { httpd_ws_send_frame_async(queued.server, queued.fd, &mut queued.frame) }
    };
    let _ = queued.done.send(result);
}

/// Sends a frame from the live thread through the server's task, and waits for it. A failed send
/// (5 s without progress included) is `false`.
#[allow(unsafe_code)]
fn send(live: &Live, fd: i32, closed: &Arc<AtomicBool>, kind: Frame, payload: &[u8]) -> bool {
    if closed.load(Ordering::SeqCst) {
        return false;
    }
    let (done, result) = sync_channel(1);
    let argument = Box::into_raw(Box::new(Queued {
        server: live.server,
        fd,
        closed: Arc::clone(closed),
        frame: raw_frame(kind, payload),
        done,
    }));
    // SAFETY: the server handle is live; `send_queued` takes the box back, or, when the work
    // cannot be queued, it is taken back here.
    unsafe {
        if httpd_queue_work(live.server, Some(send_queued), argument.cast()) != ESP_OK {
            drop(Box::from_raw(argument));
            return false;
        }
    }
    // No timeout here: `payload` is borrowed by the queued frame until `send_queued` has run, and
    // this wait is what keeps it alive (ESP-IDF's own send timeout, 5 s, bounds the send).
    result.recv() == Ok(ESP_OK)
}

/// Sends a client its waiting events as one text message in fragments (`[`, each event, `]`),
/// taken with `begin_batch` and `pop`: no batch string is built.
fn send_batch(live: &Live, fd: i32, client: ClientId, closed: &Arc<AtomicBool>) -> bool {
    if live.hub.begin_batch(client) == 0 {
        return true;
    }
    let mut first = true;
    while let Some(json) = live.hub.pop(client) {
        let opened = if first {
            send(live, fd, closed, Frame::Text(true), b"[")
        } else {
            send(live, fd, closed, Frame::Continue(false), b",")
        };
        first = false;
        if !opened || !send(live, fd, closed, Frame::Continue(false), json.as_bytes()) {
            return false;
        }
    }
    // An emptied batch started nothing.
    first || send(live, fd, closed, Frame::Continue(true), b"]")
}

/// A live session to close, checked on the server's task.
struct Closing {
    fd: i32,
    closed: Arc<AtomicBool>,
}

/// Runs on the server's task: shuts the socket down unless ESP-IDF has deleted the session
/// meanwhile. The check and the shutdown run on the task that deletes sessions, so the socket
/// number is still this session's; ESP-IDF then reads the end of the stream and closes it itself.
/// (`httpd_sess_trigger_close` would close the server's slot one loop turn later, when it may
/// already hold a new connection.)
#[allow(unsafe_code)]
extern "C" fn close_queued(argument: *mut c_void) {
    // SAFETY: `argument` is the `Box<Closing>` leaked by `close_open`, taken back once here.
    let closing = unsafe { Box::from_raw(argument.cast::<Closing>()) };
    if !closing.closed.load(Ordering::SeqCst) {
        // SAFETY: on the server's task, for a socket that is still this session's.
        unsafe { lwip_shutdown(closing.fd, SHUT_RDWR as i32) };
    }
}

/// Closes a live session's socket from the live thread, through the server's task: lwIP reuses a
/// closed socket's number at once, so the check that the session is still open must run where
/// sessions are deleted.
#[allow(unsafe_code)]
fn close_open(live: &Live, fd: i32, closed: &Arc<AtomicBool>) {
    if closed.load(Ordering::SeqCst) {
        return;
    }
    let argument = Box::into_raw(Box::new(Closing {
        fd,
        closed: Arc::clone(closed),
    }));
    // SAFETY: the server handle is live; `close_queued` takes the box back, or, when the work
    // cannot be queued, it is taken back here.
    unsafe {
        if httpd_queue_work(live.server, Some(close_queued), argument.cast()) != ESP_OK {
            drop(Box::from_raw(argument));
        }
    }
}

/// Every `TICK_MS`: for each client, a close asked by the hub, its batch, a ping every
/// `PING_SECONDS`. The table's lock is never held while sending (the server's task, which sends,
/// takes it when a socket closes).
fn live_loop() {
    let Some(live) = LIVE.get() else {
        return;
    };
    let mut last_ping = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(TICK_MS));
        let ping = last_ping.elapsed() >= Duration::from_secs(PING_SECONDS);
        if ping {
            last_ping = Instant::now();
        }
        let clients: Vec<(i32, ClientId, Arc<AtomicBool>)> = lock(&SESSIONS)
            .iter()
            .map(|s| (s.fd, s.client, Arc::clone(&s.closed)))
            .collect();
        for (fd, client, closed) in clients {
            if let Some(code) = live.hub.close_request(client) {
                let _ = send(
                    live,
                    fd,
                    &closed,
                    Frame::Close,
                    &close_payload(code, close_reason(code)),
                );
                close_open(live, fd, &closed);
                continue;
            }
            let sent = send_batch(live, fd, client, &closed)
                && (!ping || send(live, fd, &closed, Frame::Ping, &[]));
            if !sent {
                // Its slot is freed when ESP-IDF deletes the socket (`session_closed`).
                close_open(live, fd, &closed);
            }
        }
    }
}

/// Registers `/ws` on the server and starts the live thread on core 0.
#[allow(unsafe_code)]
pub fn register(
    server: httpd_handle_t,
    hub: LiveHandle,
    name: &'static str,
    port: u16,
) -> Result<(), String> {
    let live = Live {
        hub,
        name,
        port,
        server,
    };
    if LIVE.set(live).is_err() {
        return Err("the live events are already started".to_string());
    }
    lock(&SESSIONS).reserve_exact(MAX_CLIENTS);
    let conf = httpd_uri_t {
        uri: c"/ws".as_ptr(),
        method: http_method_HTTP_GET,
        handler: Some(on_frame),
        user_ctx: std::ptr::null_mut(),
        is_websocket: true,
        handle_ws_control_frames: false,
        supported_subprotocol: SUBPROTOCOL.as_ptr(),
        ws_post_handshake_cb: Some(after_handshake),
    };
    // SAFETY: the server handle is live; ESP-IDF copies the description; the path and the
    // subprotocol are static.
    let code = unsafe { httpd_register_uri_handler(server, &conf) };
    if code != ESP_OK {
        return Err(format!("cannot register /ws (error {code})"));
    }
    ThreadSpawnConfiguration {
        name: Some(c"live"),
        stack_size: LIVE_STACK_BYTES,
        pin_to_core: Some(Core::Core0),
        ..Default::default()
    }
    .set()
    .map_err(|e| format!("cannot configure the live thread: {e}"))?;
    let spawned = std::thread::Builder::new()
        .name("live".into())
        .stack_size(LIVE_STACK_BYTES)
        .spawn(live_loop);
    let _ = ThreadSpawnConfiguration::default().set();
    spawned
        .map(|_| ())
        .map_err(|e| format!("cannot start the live thread: {e}"))
}
