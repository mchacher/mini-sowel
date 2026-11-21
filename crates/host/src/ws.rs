//! Sowel's WebSocket `/ws` for the host (spec 013): the upgrade, the checks around it, and one
//! thread for each client. The core decides what a client gets (`LiveHub`: topics, queue, dedup,
//! bounds); this file moves bytes. A client's thread wakes at least every 200 ms (its socket has
//! a read timeout of that), flushes the batch of events waiting as one fragmented text message
//! written straight from the queue (one event alive at a time, no copy), pings every 30 s, and drops a
//! client from which nothing came for 60 s. A write gives up after 5 s and drops the client
//! without a close frame. At most four clients; a client holds a thread of its own, no HTTP
//! worker.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use solite_core::api::host_allowed;
use solite_core::live::{ClientId, LiveHandle};
use tungstenite::protocol::frame::coding::{CloseCode, Data, OpCode};
use tungstenite::protocol::frame::{CloseFrame, Frame};
use tungstenite::protocol::{Role, WebSocket, WebSocketConfig};
use tungstenite::{Bytes, Error, Message};

/// A client's thread wakes at least this often, in milliseconds: the read timeout of its socket,
/// and the interval of its flushes.
pub const TICK_MS: u64 = 200;
/// A write that takes longer drops the client, in seconds.
pub const WRITE_TIMEOUT_SECONDS: u64 = 5;
/// Pings are sent this often, in seconds, unless `SOLITE_WS_PING_SECONDS` says otherwise (for
/// the tests).
pub const PING_SECONDS: u64 = 30;
/// A client from which nothing arrived for this long is dropped, in seconds, unless
/// `SOLITE_WS_SILENCE_SECONDS` says otherwise (for the tests).
pub const SILENCE_SECONDS: u64 = 60;
/// Incoming messages and frames are refused (close 1009) past this many bytes.
pub const MAX_INCOMING_BYTES: usize = 1024;
/// The send buffer of a client's socket, so that a client that does not read is noticed within
/// a few batches and the kernel keeps little for it.
const SEND_BUFFER_BYTES: usize = 16 * 1024;
/// The read buffer of a client's WebSocket (an incoming frame is at most `MAX_INCOMING_BYTES`).
const READ_BUFFER_BYTES: usize = 2048;
/// Frames are written out once this many bytes are buffered.
const WRITE_BUFFER_BYTES: usize = 1024;
/// An event is written in frames of at most this many bytes, straight from the queue's text.
const FRAGMENT_BYTES: usize = 1024;
/// The write buffer holds at most `WRITE_BUFFER_BYTES` plus one more frame (a fragment, its
/// header, a pong or a close frame): well under this.
const MAX_WRITE_BUFFER_BYTES: usize = 4 * 1024;
/// How long a closing client is given to answer the close frame, in milliseconds.
const CLOSE_GRACE_MS: u64 = 1000;
/// The development server of the interface (B8), accepted as an origin besides the host's own.
const DEV_ORIGINS: [&str; 2] = ["http://localhost:5173", "http://127.0.0.1:5173"];
const WELCOME: &str =
    r#"{"type":"connected","message":"Connected to Sowel engine","version":"0.1.0"}"#;

/// The ping and silence delays.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub ping: Duration,
    pub silence: Duration,
}

impl Timing {
    /// The defaults, or the number of seconds the two variables give (for the tests).
    pub fn from_env() -> Timing {
        let seconds = |name: &str, default: u64| {
            let value = std::env::var(name)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| *v >= 1)
                .unwrap_or(default);
            Duration::from_secs(value)
        };
        Timing {
            ping: seconds("SOLITE_WS_PING_SECONDS", PING_SECONDS),
            silence: seconds("SOLITE_WS_SILENCE_SECONDS", SILENCE_SECONDS),
        }
    }
}

/// What the client threads share with the server.
#[derive(Clone)]
pub struct Context {
    pub hub: LiveHandle,
    /// The port the API listens on (the `Host` check).
    pub port: u16,
    /// Set when the host stops: each client is sent close 1001.
    pub stop: Arc<AtomicBool>,
    /// The clients running now.
    pub active: Arc<AtomicUsize>,
    pub timing: Timing,
}

/// The head of a request, as `httparse` read it.
pub struct Head {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
}

impl Head {
    /// The value of a header (the first one of that name, whatever its case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Every value of a header, in order.
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        self.headers
            .iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Reads a request head (up to its blank line) from bytes already read.
pub fn parse_head(bytes: &[u8]) -> Option<Head> {
    let mut headers = [httparse::EMPTY_HEADER; 48];
    let mut request = httparse::Request::new(&mut headers);
    match request.parse(bytes) {
        Ok(httparse::Status::Complete(_)) => Some(Head {
            method: request.method?.to_string(),
            path: request.path?.to_string(),
            headers: request
                .headers
                .iter()
                .map(|h| {
                    (
                        h.name.to_string(),
                        String::from_utf8_lossy(h.value).trim().to_string(),
                    )
                })
                .collect(),
        }),
        _ => None,
    }
}

/// True for the path `/ws`, a query string allowed.
pub fn is_ws_path(path: &str) -> bool {
    path.split('?').next() == Some("/ws")
}

/// Writes a JSON error answer and closes the connection.
pub fn refuse(stream: &mut TcpStream, status: u16, reason: &str, message: &str, extra: &str) {
    let body = format!(r#"{{"error":"{message}"}}"#);
    let answer = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(answer.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

/// Sowel's origin rule (`src/api/websocket.ts` W:48-68): the `Origin` is absent, or its
/// `host:port` is the `Host` header's; the interface's development server is accepted too.
pub fn origin_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let origin = origin.trim();
    if DEV_ORIGINS.contains(&origin) {
        return true;
    }
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = authority.trim_end_matches('/');
    !authority.is_empty()
        && !authority.contains('/')
        && host.is_some_and(|h| h.trim().eq_ignore_ascii_case(authority))
}

/// The subprotocol to echo: the first offered that starts with `bearer.` (the browser drops the
/// socket if it is not echoed; the token itself is not checked, decision D13).
pub fn subprotocol(head: &Head) -> Option<String> {
    head.all("Sec-WebSocket-Protocol")
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .find(|p| p.starts_with("bearer."))
        .map(str::to_string)
}

/// True when the header's comma-separated value has this token (without case).
fn has_token(value: Option<&str>, token: &str) -> bool {
    value.is_some_and(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
}

/// Answers a `/ws` request: the refusals before the upgrade (403, 405, 426, 400, 503), else the
/// upgrade and the client's session, run in this thread until the client is gone. `rest` is what
/// was read after the head.
pub fn serve_request(mut stream: TcpStream, head: &Head, rest: Vec<u8>, ctx: &Context) {
    if !host_allowed(head.header("Host"), ctx.port) {
        return refuse(&mut stream, 403, "Forbidden", "Forbidden host", "");
    }
    if head.method != "GET" {
        return refuse(
            &mut stream,
            405,
            "Method Not Allowed",
            "Method not allowed",
            "Allow: GET\r\n",
        );
    }
    let key = head
        .header("Sec-WebSocket-Key")
        .filter(|k| !k.is_empty())
        .map(str::to_string);
    let upgrade = has_token(head.header("Upgrade"), "websocket")
        && has_token(head.header("Connection"), "upgrade");
    let Some(key) = key.filter(|_| upgrade) else {
        return refuse(
            &mut stream,
            426,
            "Upgrade Required",
            "Upgrade Required",
            "Upgrade: websocket\r\n",
        );
    };
    if head.header("Sec-WebSocket-Version") != Some("13") {
        return refuse(
            &mut stream,
            400,
            "Bad Request",
            "Unsupported WebSocket version",
            "Sec-WebSocket-Version: 13\r\n",
        );
    }
    let Some(client) = ctx.hub.connect() else {
        return refuse(&mut stream, 503, "Service Unavailable", "Busy", "");
    };
    // From here the slot is freed, whatever happens, by the guard.
    let guard = Session::new(ctx, client);
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let mut answer = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n"
    );
    if let Some(protocol) = subprotocol(head) {
        answer.push_str(&format!("Sec-WebSocket-Protocol: {protocol}\r\n"));
    }
    answer.push_str("\r\n");
    if configure(&stream).is_err()
        || stream.write_all(answer.as_bytes()).is_err()
        || stream.flush().is_err()
    {
        return;
    }
    let config = WebSocketConfig::default()
        .read_buffer_size(READ_BUFFER_BYTES)
        .write_buffer_size(WRITE_BUFFER_BYTES)
        .max_write_buffer_size(MAX_WRITE_BUFFER_BYTES)
        .max_message_size(Some(MAX_INCOMING_BYTES))
        .max_frame_size(Some(MAX_INCOMING_BYTES));
    let socket = if rest.is_empty() {
        WebSocket::from_raw_socket(stream, Role::Server, Some(config))
    } else {
        WebSocket::from_partially_read(stream, rest, Role::Server, Some(config))
    };
    let allowed = origin_allowed(head.header("Origin"), head.header("Host"));
    session(socket, &guard, allowed);
}

/// The timeouts and buffers of a client's socket.
fn configure(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(TICK_MS)))?;
    stream.set_write_timeout(Some(Duration::from_secs(WRITE_TIMEOUT_SECONDS)))?;
    stream.set_nodelay(true)?;
    socket2::SockRef::from(stream).set_send_buffer_size(SEND_BUFFER_BYTES)
}

/// A client's place in the hub and in the count of running clients, given back on drop.
struct Session<'a> {
    ctx: &'a Context,
    client: ClientId,
}

impl<'a> Session<'a> {
    fn new(ctx: &'a Context, client: ClientId) -> Self {
        ctx.active.fetch_add(1, Ordering::SeqCst);
        Session { ctx, client }
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        self.ctx.hub.disconnect(self.client);
        self.ctx.active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn is_timeout(error: &Error) -> bool {
    matches!(error, Error::Io(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

/// Why a client's loop ended.
enum End {
    /// Close it with this code (after the close frame, wait for its answer).
    Close(u16),
    /// The client is gone or too slow: drop the socket.
    Drop,
}

/// The client's life: the welcome, then each tick the frames it sent and the events waiting.
fn session(mut socket: WebSocket<TcpStream>, guard: &Session<'_>, origin_ok: bool) {
    let (ctx, client) = (guard.ctx, guard.client);
    if !origin_ok {
        return finish(&mut socket, 4003, "Origin not allowed");
    }
    if socket.send(Message::Text(WELCOME.into())).is_err() {
        return;
    }
    let end = run(&mut socket, ctx, client);
    match end {
        End::Close(code) => finish(&mut socket, code, close_reason(code)),
        End::Drop => {}
    }
    let _ = socket.get_ref().shutdown(Shutdown::Both);
}

fn close_reason(code: u16) -> &'static str {
    match code {
        1001 => "Going away",
        1002 => "Protocol error",
        1007 => "Invalid payload",
        1009 => "Message too big",
        1013 => "Try again later",
        _ => "",
    }
}

/// The loop; returns when the client must be closed or dropped.
fn run(socket: &mut WebSocket<TcpStream>, ctx: &Context, client: ClientId) -> End {
    let started = Instant::now();
    let (mut last_flush, mut last_ping, mut last_heard) = (started, started, started);
    loop {
        if ctx.stop.load(Ordering::SeqCst) {
            return End::Close(1001);
        }
        if let Some(code) = ctx.hub.close_request(client) {
            return End::Close(code);
        }
        let now = Instant::now();
        if now.duration_since(last_heard) >= ctx.timing.silence {
            log::info!("websocket: client {client} silent, dropped");
            return End::Drop;
        }
        if now.duration_since(last_ping) >= ctx.timing.ping {
            last_ping = now;
            if socket.send(Message::Ping(Bytes::new())).is_err() {
                return End::Drop;
            }
        }
        if now.duration_since(last_flush) >= Duration::from_millis(TICK_MS) {
            last_flush = now;
            if send_batch(socket, ctx, client).is_err() {
                return End::Drop;
            }
        }
        match socket.read() {
            Ok(message) => {
                last_heard = Instant::now();
                match message {
                    Message::Text(text) => {
                        ctx.hub.subscribe(client, text.as_str());
                    }
                    // A binary frame is read as text (UTF-8, else ignored), as Sowel does.
                    Message::Binary(bytes) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            ctx.hub.subscribe(client, text);
                        }
                    }
                    // The pong is queued by the library: written by the next flush.
                    Message::Ping(_) => {
                        if socket.flush().is_err() {
                            return End::Drop;
                        }
                    }
                    // The library queued the close reply: write it before the socket goes.
                    Message::Close(_) => {
                        let _ = socket.flush();
                        return End::Drop;
                    }
                    Message::Pong(_) | Message::Frame(_) => {}
                }
            }
            Err(e) if is_timeout(&e) => {}
            Err(Error::Capacity(_)) => return End::Close(1009),
            Err(Error::Protocol(_)) => return End::Close(1002),
            Err(Error::Utf8(_)) => return End::Close(1007),
            Err(_) => return End::Drop,
        }
    }
}

/// A part of a queued event's text, owned by a `Bytes`.
struct Slice {
    text: Arc<str>,
    from: usize,
    to: usize,
}

impl AsRef<[u8]> for Slice {
    fn as_ref(&self) -> &[u8] {
        &self.text.as_bytes()[self.from..self.to]
    }
}

/// Writes the events waiting as one text message in fragments: `[`, the events separated by
/// commas, `]`, each event written in fragments of at most `FRAGMENT_BYTES` that share the
/// queue's text (no copy, no batch string). A write that fails (5 s without progress included) is an error.
fn send_batch(
    socket: &mut WebSocket<TcpStream>,
    ctx: &Context,
    client: ClientId,
) -> Result<(), Error> {
    let count = ctx.hub.begin_batch(client);
    if count == 0 {
        return Ok(());
    }
    let mut first = true;
    while let Some(json) = ctx.hub.pop(client) {
        let separator = Frame::message(
            Bytes::from_static(if first { b"[" } else { b"," }),
            if first {
                OpCode::Data(Data::Text)
            } else {
                OpCode::Data(Data::Continue)
            },
            false,
        );
        first = false;
        socket.write(Message::Frame(separator))?;
        // The event goes out in fragments that share its text: no copy of it is made.
        let mut at = 0;
        while at < json.len() {
            let to = (at + FRAGMENT_BYTES).min(json.len());
            let part = Bytes::from_owner(Slice {
                text: Arc::clone(&json),
                from: at,
                to,
            });
            let frame = Frame::message(part, OpCode::Data(Data::Continue), false);
            socket.write(Message::Frame(frame))?;
            at = to;
        }
    }
    if first {
        // The batch was emptied under us (the client's queue was freed): nothing was started.
        return Ok(());
    }
    let end = Frame::message(Bytes::from_static(b"]"), OpCode::Data(Data::Continue), true);
    socket.write(Message::Frame(end))?;
    socket.flush()
}

/// Sends a close frame and waits a moment for the client's, so that the frame is not lost to a
/// reset; then the socket is dropped.
fn finish(socket: &mut WebSocket<TcpStream>, code: u16, reason: &str) {
    let frame = CloseFrame {
        code: CloseCode::from(code),
        reason: reason.to_string().into(),
    };
    if socket.close(Some(frame)).is_err() {
        return;
    }
    let end = Instant::now() + Duration::from_millis(CLOSE_GRACE_MS);
    while Instant::now() < end {
        match socket.read() {
            Ok(_) => {}
            Err(e) if is_timeout(&e) => {
                if socket.flush().is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

/// Reads whatever is on the stream into `buffer` until the blank line that ends a request head,
/// or `max` bytes, or the end of the stream; false when nothing usable came.
pub fn read_head(stream: &mut TcpStream, buffer: &mut Vec<u8>, max: usize) -> bool {
    let mut chunk = [0u8; 2048];
    loop {
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            return true;
        }
        if buffer.len() >= max {
            return true;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return !buffer.is_empty(),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(_) => return !buffer.is_empty(),
        }
    }
}
