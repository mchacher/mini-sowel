//! The radio image's live events (feature `radio`, spec 023 R5): `/ws` as ESP-IDF's WebSocket
//! handler, which answers the handshake itself; then the `Host`, the `Origin` and a slot of the
//! core's `LiveHub` are checked, a refusal being a close code (4003, 4003, 1013). A live thread on
//! core 0 sends each client its batch every 200 ms as one text message in fragments, and a ping
//! every 30 s, through esp-idf-svc's detached sender. The rules are the core's (`live::session`,
//! `LiveHub`), shared with the host's `ws.rs`.

use std::ffi::CStr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use esp_idf_svc::hal::cpu::Core;
use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::http::server::ws::{EspHttpWsConnection, EspHttpWsDetachedSender};
use esp_idf_svc::sys::{
    ESP_OK, EspError, httpd_handle_t, httpd_req_get_hdr_value_len, httpd_req_get_hdr_value_str,
    httpd_req_t, httpd_sess_trigger_close,
};
use esp_idf_svc::ws::FrameType;
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
const SUBPROTOCOL: &str = "bearer.none";
/// The first message, as the host sends it (spec 013, `crates/host/src/ws.rs`).
const WELCOME: &str =
    r#"{"type":"connected","message":"Connected to Sowel engine","version":"0.1.0"}"#;

/// What the handler and the live thread share.
struct Live {
    hub: LiveHandle,
    name: &'static str,
    port: u16,
    server: httpd_handle_t,
}

// SAFETY: the server handle is an opaque pointer ESP-IDF lets any task use for its queued calls
// (`httpd_sess_trigger_close`, the detached sender's `httpd_queue_work`).
#[allow(unsafe_code)]
unsafe impl Send for Live {}
// SAFETY: as above; the hub is itself `Sync`.
#[allow(unsafe_code)]
unsafe impl Sync for Live {}

static LIVE: OnceLock<Live> = OnceLock::new();

/// One live client: its socket, its sender and its id in the hub, the message being received.
struct Session {
    fd: i32,
    client: ClientId,
    sender: EspHttpWsDetachedSender,
    incoming: Vec<u8>,
}

/// The sessions, at most `MAX_CLIENTS` (the hub refuses a 5th).
static SESSIONS: Mutex<Vec<Session>> = Mutex::new(Vec::new());

/// `Host` and `Origin` of the handshake, copied while the request is valid.
#[allow(unsafe_code)]
fn handshake_headers(request: *mut httpd_req_t) -> (Option<String>, Option<String>) {
    let read = |name: &CStr| -> Option<String> {
        // SAFETY: `request` is the handshake's request, valid during this handler call (the
        // `New` variant); `name` is NUL-terminated; ESP-IDF writes at most `buffer.len()` bytes,
        // NUL included, into `buffer`, which outlives the call. Both values are copied out.
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

/// Closes a live session's socket from the live thread, unless the session is already closed:
/// lwIP reuses a closed socket's number at once, and closing it again would close whichever
/// connection holds that number now (a reload's new one, for example).
fn close_open(server: httpd_handle_t, fd: i32, sender: &EspHttpWsDetachedSender) {
    if !sender.is_closed() {
        trigger_close(server, fd);
    }
}

/// Sends a close frame on the handler's connection and closes the socket.
fn refuse(connection: &mut EspHttpWsConnection, server: httpd_handle_t, code: u16, reason: &str) {
    let _ = connection.send(FrameType::Close, &close_payload(code, reason));
    trigger_close(server, connection.session());
}

/// The handler: a new client (checks, slot, welcome), a frame (a subscribe, or 1009 past 1 KB),
/// a socket closed (its slot freed).
fn handle(connection: &mut EspHttpWsConnection) -> Result<(), EspError> {
    let Some(live) = LIVE.get() else {
        return Ok(());
    };
    match connection {
        EspHttpWsConnection::New(_, request) => {
            let (host, origin) = handshake_headers(*request);
            if !host_allowed(host.as_deref(), live.port, Reach::Lan { name: live.name }) {
                refuse(connection, live.server, 4003, "Forbidden host");
                return Ok(());
            }
            if !origin_allowed(origin.as_deref(), host.as_deref()) {
                refuse(connection, live.server, 4003, "Origin not allowed");
                return Ok(());
            }
            let Some(client) = live.hub.connect() else {
                refuse(connection, live.server, 1013, close_reason(1013));
                return Ok(());
            };
            let sender = match connection.create_detached_sender() {
                Ok(sender) => sender,
                Err(e) => {
                    live.hub.disconnect(client);
                    return Err(e);
                }
            };
            lock(&SESSIONS).push(Session {
                fd: connection.session(),
                client,
                sender,
                incoming: Vec::new(),
            });
            connection.send(FrameType::Text(false), WELCOME.as_bytes())
        }
        EspHttpWsConnection::Receiving(..) => receive(connection, live),
        EspHttpWsConnection::Closed(fd) => {
            let fd = *fd;
            let mut sessions = lock(&SESSIONS);
            if let Some(at) = sessions.iter().position(|s| s.fd == fd) {
                let session = sessions.swap_remove(at);
                live.hub.disconnect(session.client);
            }
            Ok(())
        }
    }
}

/// One data frame: kept until its message ends, then a subscribe; past `MAX_INCOMING_BYTES`, the
/// client is closed with 1009.
fn receive(connection: &mut EspHttpWsConnection, live: &Live) -> Result<(), EspError> {
    // A text frame's length counts a NUL that ESP-IDF adds.
    let mut buffer = [0u8; MAX_INCOMING_BYTES + 1];
    let (kind, length) = connection.recv(&mut buffer)?;
    let fd = connection.session();
    if length > buffer.len() {
        too_big(connection, live, fd);
        return Ok(());
    }
    let (data, ends) = match kind {
        FrameType::Text(fragmented) => (&buffer[..length.saturating_sub(1)], !fragmented),
        FrameType::Binary(fragmented) => (&buffer[..length], !fragmented),
        FrameType::Continue(last) => (&buffer[..length], last),
        // ESP-IDF answers the control frames itself.
        _ => return Ok(()),
    };
    let message = {
        let mut sessions = lock(&SESSIONS);
        let Some(session) = sessions.iter_mut().find(|s| s.fd == fd) else {
            return Ok(());
        };
        if session.incoming.len() + data.len() > MAX_INCOMING_BYTES {
            session.incoming.clear();
            drop(sessions);
            too_big(connection, live, fd);
            return Ok(());
        }
        session.incoming.extend_from_slice(data);
        if !ends {
            return Ok(());
        }
        (session.client, std::mem::take(&mut session.incoming))
    };
    let (client, bytes) = message;
    live.hub.subscribe(client, &String::from_utf8_lossy(&bytes));
    Ok(())
}

fn too_big(connection: &mut EspHttpWsConnection, live: &Live, fd: i32) {
    let _ = connection.send(FrameType::Close, &close_payload(1009, close_reason(1009)));
    trigger_close(live.server, fd);
}

/// Sends a client its waiting events as one text message in fragments (`[`, each event, `]`),
/// taken with `begin_batch` and `pop`: no batch string is built. A failed send (5 s without
/// progress included) is the error.
fn send_batch(
    live: &Live,
    client: ClientId,
    sender: &mut EspHttpWsDetachedSender,
) -> Result<(), EspError> {
    if live.hub.begin_batch(client) == 0 {
        return Ok(());
    }
    let mut first = true;
    while let Some(json) = live.hub.pop(client) {
        if first {
            sender.send(FrameType::Text(true), b"[")?;
        } else {
            sender.send(FrameType::Continue(false), b",")?;
        }
        first = false;
        sender.send(FrameType::Continue(false), json.as_bytes())?;
    }
    if first {
        // The batch was emptied under us: nothing was started.
        return Ok(());
    }
    sender.send(FrameType::Continue(true), b"]")
}

/// Every `TICK_MS`: for each client, a close asked by the hub, its batch, a ping every
/// `PING_SECONDS`. The table's lock is never held while sending (the sender waits for the
/// server's task, whose handler takes the lock).
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
        let clients: Vec<(i32, ClientId, EspHttpWsDetachedSender)> = lock(&SESSIONS)
            .iter()
            .map(|s| (s.fd, s.client, s.sender.clone()))
            .collect();
        for (fd, client, mut sender) in clients {
            if let Some(code) = live.hub.close_request(client) {
                let _ = sender.send(FrameType::Close, &close_payload(code, close_reason(code)));
                close_open(live.server, fd, &sender);
                continue;
            }
            let sent = send_batch(live, client, &mut sender).and_then(|()| {
                if ping {
                    sender.send(FrameType::Ping, &[])
                } else {
                    Ok(())
                }
            });
            if sent.is_err() {
                // Dropped: the close callback frees its slot.
                close_open(live.server, fd, &sender);
            }
        }
    }
}

/// Registers `/ws` on the server and starts the live thread on core 0.
pub fn register(
    server: &mut EspHttpServer<'static>,
    hub: LiveHandle,
    name: &'static str,
    port: u16,
) -> Result<(), String> {
    use esp_idf_svc::handle::RawHandle;
    let live = Live {
        hub,
        name,
        port,
        server: server.handle(),
    };
    if LIVE.set(live).is_err() {
        return Err("the live events are already started".to_string());
    }
    lock(&SESSIONS).reserve_exact(MAX_CLIENTS);
    server
        .ws_handler("/ws", Some(SUBPROTOCOL), handle)
        .map_err(|e| format!("cannot register /ws: {e}"))?;
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
