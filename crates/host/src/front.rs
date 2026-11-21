//! The door of the API: the port the host listens on is owned here, not by `tiny_http`, because a
//! WebSocket client needs a socket that wakes up every 200 ms and gives up writes after 5 s, and
//! `tiny_http` hands an upgraded connection over as a boxed stream whose timeouts cannot be set.
//! The obvious way, a read timeout on the listener that accepted sockets inherit, breaks Linux:
//! `accept` obeys it too and fails every 200 ms, which ends `tiny_http`'s accept loop (checked
//! in a Docker container; macOS ignores it for `accept`). So each accepted connection is looked
//! at here: a request for `/ws` stays in this program (`ws.rs`, on the socket we own), and
//! every other request is passed on, byte for byte, to the `tiny_http` server that listens on a
//! loopback port of its own and answers the API as before.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use crate::ws::{self, Context};

/// Connections looked at or passed on at the same time; one more is closed at once.
const MAX_CONNECTIONS: usize = 64;
/// A request head is looked for in this many bytes.
const MAX_HEAD_BYTES: usize = 16 * 1024;
/// A client has this long to send its request head, in seconds.
const HEAD_TIMEOUT_SECONDS: u64 = 10;
/// A passed-on connection is closed after this long without a byte from the client, in seconds
/// (a client that stalls in its body holds a thread no longer than this).
const IDLE_TIMEOUT_SECONDS: u64 = 30;
/// A write to a client or to the API's server gives up after this long, in seconds.
const WRITE_TIMEOUT_SECONDS: u64 = 5;

/// The pause after an `accept` that failed, in milliseconds.
const ACCEPT_RETRY_MS: u64 = 100;

/// What the door needs.
pub struct Door {
    pub listener: TcpListener,
    /// The `tiny_http` server's loopback address.
    pub backend: SocketAddr,
    pub ws: Context,
    /// Set to end the accept loop.
    pub stop: Arc<AtomicBool>,
}

/// Accepts connections until `stop` is set; each one is handled in its own thread.
pub fn run(door: &Door) {
    let busy = Arc::new(AtomicUsize::new(0));
    for stream in door.listener.incoming() {
        if door.stop.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else {
            // A listener that keeps failing (no file descriptors left...) must not spin.
            thread::sleep(Duration::from_millis(ACCEPT_RETRY_MS));
            continue;
        };
        if busy.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            busy.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let (backend, ctx, count) = (door.backend, door.ws.clone(), Arc::clone(&busy));
        let spawned = thread::Builder::new().spawn(move || {
            handle(stream, backend, &ctx);
            count.fetch_sub(1, Ordering::SeqCst);
        });
        if spawned.is_err() {
            busy.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Wakes the accept loop (a connection to the door itself) so that it sees `stop`.
pub fn wake(port: u16) {
    let _ = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_secs(1),
    );
}

fn handle(mut stream: TcpStream, backend: SocketAddr, ctx: &Context) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(HEAD_TIMEOUT_SECONDS)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(WRITE_TIMEOUT_SECONDS)));
    let mut buffer = Vec::new();
    if !ws::read_head(&mut stream, &mut buffer, MAX_HEAD_BYTES) {
        return;
    }
    let head = ws::parse_head(&buffer);
    if let Some(head) = &head
        && ws::is_ws_path(&head.path)
    {
        let end = buffer
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map_or(buffer.len(), |at| at + 4);
        let rest = buffer.split_off(end);
        return ws::serve_request(stream, head, rest, ctx);
    }
    // An upgrade anywhere else would be handed to `tiny_http` as a boxed stream that pins one of
    // its workers: refused here (`/ws/`, an absolute-form target, any other path).
    let upgrade = match &head {
        Some(head) => head
            .all("Connection")
            .any(|v| v.to_ascii_lowercase().contains("upgrade")),
        None => String::from_utf8_lossy(&buffer)
            .to_ascii_lowercase()
            .contains("upgrade"),
    };
    if upgrade {
        return ws::refuse(&mut stream, 400, "Bad Request", "Upgrade only on /ws", "");
    }
    // One request for each connection: a later request of a kept-alive connection would not be
    // looked at here, so the API's server is told to close after its answer.
    match head {
        Some(_) => pass_on(stream, &close_after_answer(&buffer), backend),
        None => pass_on(stream, &buffer, backend),
    }
}

/// The request head with its `Connection` headers replaced by `Connection: close`; the rest
/// (the body bytes that were read with it included) is unchanged.
fn close_after_answer(buffer: &[u8]) -> Vec<u8> {
    let end = buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map_or(buffer.len(), |at| at + 2);
    let mut out = Vec::with_capacity(buffer.len() + 20);
    for line in buffer[..end].split_inclusive(|b| *b == b'\n') {
        let name = line.split(|b| *b == b':').next().unwrap_or_default();
        if !name.eq_ignore_ascii_case(b"connection") || line == name {
            out.extend_from_slice(line);
        }
    }
    out.extend_from_slice(b"Connection: close\r\n");
    out.extend_from_slice(&buffer[end..]);
    out
}

/// Passes the connection on to the API's server: what was read, then the bytes both ways until
/// either side ends.
fn pass_on(client: TcpStream, first: &[u8], backend: SocketAddr) {
    let Ok(mut server) = TcpStream::connect_timeout(&backend, Duration::from_secs(2)) else {
        return;
    };
    let _ = server.set_write_timeout(Some(Duration::from_secs(WRITE_TIMEOUT_SECONDS)));
    let _ = client.set_read_timeout(Some(Duration::from_secs(IDLE_TIMEOUT_SECONDS)));
    if server.write_all(first).is_err() {
        return;
    }
    let (Ok(mut from_server), Ok(mut to_client), Ok(mut from_client)) =
        (server.try_clone(), client.try_clone(), client.try_clone())
    else {
        return;
    };
    let answers = thread::Builder::new().spawn(move || {
        let _ = copy(&mut from_server, &mut to_client);
        let _ = to_client.shutdown(Shutdown::Both);
        let _ = from_server.shutdown(Shutdown::Both);
    });
    if answers.is_err() {
        return;
    }
    let _ = copy(&mut from_client, &mut server);
    // The client has nothing more to say; the answer may still be coming.
    let _ = server.shutdown(Shutdown::Write);
}

/// Copies until the reader ends or fails (a timeout included).
fn copy(from: &mut impl Read, to: &mut impl Write) -> io::Result<()> {
    let mut chunk = [0u8; 4096];
    loop {
        let count = from.read(&mut chunk)?;
        if count == 0 {
            return Ok(());
        }
        to.write_all(&chunk[..count])?;
    }
}
