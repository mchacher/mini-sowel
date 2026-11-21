//! `--serve`: resume the network, keep the devices up to date from what they send, and serve
//! Sowel's device, zone, equipment and recipe API, and its live events (`/ws`), on 127.0.0.1. The
//! radio loop runs in the calling thread and is the only writer of the files; HTTP workers answer
//! requests, with the home behind a lock that is never held during radio I/O, a file write, or the
//! wait for an order's answer. An order goes through the home's queue to the radio loop, which
//! sends it between two polls. The random source is shared and always taken after the home lock.
//!
//! The port is owned by `front.rs`, which keeps `/ws` (one thread per client, `ws.rs`) and passes
//! every other request to the `tiny_http` server of the API, on a loopback port of its own.
//!
//! Locks, in the order they may be taken: the home, then the orders' queue, then the live hub
//! (the home emits events while it is held). The random source comes after the home. A
//! WebSocket client's thread takes the hub only, never the home, so no client can hold up the
//! radio loop or the API.

use std::io::{self, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mini_sowel_core::api::{
    ListCursor, MAX_BODY_BYTES, Reply, Request, handle_stream, order_response,
};
use mini_sowel_core::devices::lock;
use mini_sowel_core::home::Home;
use mini_sowel_core::orders::OrderRunner;
use mini_sowel_core::radio::{Radio, RadioTransport, identity_lines, network_line};
use tiny_http::{Header, Response, Server};

use crate::front::{self, Door};
use crate::random::SystemRandom;
use crate::static_files;
use crate::store::{FileStore, Saver};
use crate::ws::{Context, Timing};
use crate::{backup, clock};

/// The server, so that Ctrl-C can unblock the HTTP thread.
static SERVER: OnceLock<Arc<Server>> = OnceLock::new();
/// The door's stop flag and port, so that Ctrl-C can wake its accept loop.
static DOOR: OnceLock<(Arc<AtomicBool>, u16)> = OnceLock::new();

/// Wakes the HTTP thread and the door so that they end; called by the Ctrl-C handler. Does
/// nothing before they exist (`serve` checks the stop flag once they do).
pub fn unblock() {
    if let Some(server) = SERVER.get() {
        server.unblock();
    }
    if let Some((stop, port)) = DOOR.get() {
        stop.store(true, Ordering::SeqCst);
        front::wake(*port);
    }
}

/// Milliseconds since 1970.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Milliseconds on a monotonic clock, from the first call: the clock of the orders' deadlines,
/// which the HTTP waiter (an `Instant`) shares. The wall clock (`now_ms`) can jump; it stays for
/// the timestamps of data.
pub fn monotonic_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    u64::try_from(START.get_or_init(Instant::now).elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Binds the port on 127.0.0.1 only: the door's listener (the port asked for), and the API's
/// `tiny_http` server on a loopback port of its own, whose address is returned.
fn bind(port: u16) -> Result<(TcpListener, Arc<Server>, std::net::SocketAddr), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("serve: cannot listen on 127.0.0.1:{port}: {e}"))?;
    let server = Server::http(("127.0.0.1", 0))
        .map(Arc::new)
        .map_err(|e| format!("serve: cannot start the API's server: {e}"))?;
    let backend = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| "serve: the API's server has no address".to_string())?;
    Ok((listener, server, backend))
}

/// How long the host waits for the WebSocket clients to answer their close frame when it stops.
const CLIENTS_GRACE: Duration = Duration::from_secs(3);

/// Requests answered at the same time; one more is told to come back (503).
const MAX_WORKERS: usize = 4;

/// A streamed list as a `Read`: each time the buffer is empty it takes the home, writes the next
/// chunk of the list into the buffer, and lets go of the home. The home is never held while the
/// socket is written (the server drains the reader between two chunks).
struct ChunkReader<'a> {
    home: &'a Mutex<Home>,
    cursor: ListCursor,
    buffer: Vec<u8>,
    position: usize,
}

impl Read for ChunkReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.position >= self.buffer.len() {
            // An error aborts the answer: the client sees a broken stream, not a short list.
            let chunk = self.cursor.next_chunk(&lock(self.home))?;
            match chunk {
                Some(chunk) => {
                    self.buffer = chunk.into_bytes();
                    self.position = 0;
                }
                None => return Ok(0),
            }
        }
        let count = out.len().min(self.buffer.len() - self.position);
        out[..count].copy_from_slice(&self.buffer[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

/// Reads one request, asks the core, answers. Runs in a worker thread: a client that stalls
/// while sending its body holds this thread only.
fn answer(
    mut request: tiny_http::Request,
    home: &Mutex<Home>,
    random: &Mutex<SystemRandom>,
    port: u16,
    content_type: &Header,
    ui: &Path,
) {
    let method = request.method().to_string();
    let path = request.url().to_string();
    let host = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Host"))
        .map(|h| h.value.to_string());
    let request_type = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Content-Type"))
        .map(|h| h.value.to_string());
    // One byte more than allowed is enough to see that the body is too long.
    let mut body = Vec::new();
    let _ = request
        .as_reader()
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body);
    let reply = {
        let mut guard = lock(home);
        let mut fill = |buffer: &mut [u8]| lock(random).fill(buffer);
        handle_stream(
            &mut guard,
            &Request {
                method: &method,
                path: &path,
                host: host.as_deref(),
                port,
                content_type: request_type.as_deref(),
                body: &body,
            },
            now_ms(),
            &mut fill,
        )
    };
    // The interface's files do not need the home: its lock is already let go.
    if let Reply::Static(file) = &reply {
        return static_files::send(request, ui, file);
    }
    if let Reply::StaticError(response) = reply {
        return static_files::send_refusal(request, response.status, response.body);
    }
    // An order waits for the radio loop's answer, without the lock.
    let reply = match reply {
        Reply::Order(wait) => Reply::Whole(order_response(wait)),
        other => other,
    };
    // The client may have gone: nothing to do about it.
    let _ = match reply {
        // Unknown length: sent chunked, one chunk of the list at a time.
        Reply::List(cursor) => request.respond(tiny_http::Response::new(
            tiny_http::StatusCode(200),
            vec![content_type.clone()],
            ChunkReader {
                home,
                cursor,
                buffer: Vec::new(),
                position: 0,
            },
            None,
            None,
        )),
        Reply::Order(_) | Reply::Static(_) | Reply::StaticError(_) => {
            request.respond(Response::empty(500))
        }
        Reply::Whole(response) => match response.body {
            Some(text) => request.respond(
                Response::from_string(text)
                    .with_status_code(response.status)
                    .with_header(content_type.clone()),
            ),
            None => request.respond(Response::empty(response.status)),
        },
    };
}

/// Accepts requests and hands each to a worker thread (at most `MAX_WORKERS` at once), so that
/// a stalled client never blocks the others or the exit.
fn http_loop(
    server: &Server,
    home: &Arc<Mutex<Home>>,
    random: &Arc<Mutex<SystemRandom>>,
    port: u16,
    content_type: &Header,
    ui: &Arc<PathBuf>,
) {
    let busy = Arc::new(AtomicUsize::new(0));
    for request in server.incoming_requests() {
        if busy.fetch_add(1, Ordering::SeqCst) >= MAX_WORKERS {
            busy.fetch_sub(1, Ordering::SeqCst);
            let _ = request.respond(
                Response::from_string(r#"{"error":"Busy"}"#)
                    .with_status_code(503)
                    .with_header(content_type.clone()),
            );
            continue;
        }
        let (home, random, worker_busy, content_type, ui) = (
            Arc::clone(home),
            Arc::clone(random),
            Arc::clone(&busy),
            content_type.clone(),
            Arc::clone(ui),
        );
        let spawned = thread::Builder::new().spawn(move || {
            answer(request, &home, &random, port, &content_type, &ui);
            worker_busy.fetch_sub(1, Ordering::SeqCst);
        });
        if spawned.is_err() {
            busy.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Runs until Ctrl-C. `Ok(true)` when Ctrl-C stopped it.
pub fn serve<T: RadioTransport>(
    transport: T,
    label: &str,
    port: u16,
    verbose: bool,
    data: &std::path::Path,
    ui: &Path,
    stop: &AtomicBool,
) -> Result<bool, String> {
    let store = FileStore::new(data);
    let home = Arc::new(Mutex::new(store.load_home(now_ms())?));
    {
        // The recipes' timers and the orders' deadlines run on the same monotonic clock from the
        // first moment, API calls included.
        let mut guard = lock(&home);
        guard.orders().set_clock(Arc::new(monotonic_ms));
        // The recipe instances start once the home is loaded: their start sync queues its
        // orders, which wait for the radio.
        guard.start_recipes(monotonic_ms(), now_ms());
    }
    let random = Arc::new(Mutex::new(SystemRandom::open()?));
    let (listener, server, backend) = bind(port)?;
    let _ = SERVER.set(Arc::clone(&server));
    let door_stop = Arc::new(AtomicBool::new(false));
    let _ = DOOR.set((Arc::clone(&door_stop), port));
    let clients_stop = Arc::new(AtomicBool::new(false));
    let clients = Arc::new(AtomicUsize::new(0));
    let door = Door {
        listener,
        backend,
        ws: Context {
            hub: lock(&home).live(),
            port,
            stop: Arc::clone(&clients_stop),
            active: Arc::clone(&clients),
            timing: Timing::from_env(),
        },
        stop: Arc::clone(&door_stop),
    };
    thread::spawn(move || front::run(&door));
    let content_type = Header::from_bytes("Content-Type", "application/json; charset=utf-8")
        .map_err(|()| "serve: cannot build the Content-Type header".to_string())?;
    {
        let (server, home, random) = (Arc::clone(&server), Arc::clone(&home), Arc::clone(&random));
        let ui = Arc::new(ui.to_path_buf());
        thread::spawn(move || http_loop(&server, &home, &random, port, &content_type, &ui));
    }
    // Ctrl-C before the server existed: nobody unblocked it.
    if stop.load(Ordering::SeqCst) {
        unblock();
    }
    if !static_files::is_built(ui) {
        println!("{}", static_files::missing_line(ui));
    }
    let mut saver = Saver::new(store, now_ms());
    let result = radio_loop(
        transport, label, port, verbose, data, stop, &home, &random, &mut saver,
    );
    // The WebSocket clients are told (close 1001) as the host stops. The store is written
    // first, and the HTTP threads are never waited for: a client that stalls cannot keep the
    // host from saving and exiting; the WebSocket clients are given a moment to answer.
    clients_stop.store(true, Ordering::SeqCst);
    unblock();
    let saved = saver.tick(&home, now_ms(), true);
    let waited = Instant::now();
    while clients.load(Ordering::SeqCst) > 0 && waited.elapsed() < CLIENTS_GRACE {
        thread::sleep(Duration::from_millis(20));
    }
    result.and_then(|stopped| saved.map(|()| stopped))
}

#[allow(clippy::too_many_arguments)]
fn radio_loop<T: RadioTransport>(
    transport: T,
    label: &str,
    port: u16,
    verbose: bool,
    data: &std::path::Path,
    stop: &AtomicBool,
    home: &Mutex<Home>,
    random: &Mutex<SystemRandom>,
    saver: &mut Saver,
) -> Result<bool, String> {
    let mut fill = |buffer: &mut [u8]| lock(random).fill(buffer);
    let mut radio = Radio::new(transport, clock());
    let identity = radio.identify().map_err(|e| format!("radio: {e}"))?;
    let lines = identity_lines(label, &identity);
    // The `network:` line of the identification is left out, as with `--listen`.
    for line in &lines[..3] {
        println!("{line}");
    }
    let network = radio.listen().map_err(|e| format!("radio: {e}"))?;
    let foreign = !backup::matches(data, &network);
    println!("{}", network_line(&network, foreign));
    println!("serving: http://127.0.0.1:{port}");
    // The orders of the API (and of the core) wait in this queue; the radio loop is the only one
    // that sends them.
    let orders = lock(home).orders().clone();
    orders.set_clock(Arc::new(monotonic_ms));
    let mut runner = OrderRunner::new();
    while !stop.load(Ordering::SeqCst) {
        let events = radio.poll().map_err(|e| format!("radio: {e}"))?;
        let activity = radio.take_activity();
        let routes = radio.take_routes();
        let now = now_ms();
        {
            let mut guard = lock(home);
            for item in &activity {
                guard.devices.apply_activity(item, now);
            }
            for update in &routes {
                guard.devices.set_route(update);
            }
            for event in &events {
                guard.measures(event, now, &mut fill)?;
            }
            let status_changed = guard.devices.refresh_status(now);
            // The zone aggregation (and the motion times) after each batch of measures, and
            // when silence alone turned a device offline.
            if status_changed || !events.is_empty() || !activity.is_empty() {
                guard.recompute(now);
            }
            // What goes stale with time alone is seen at least once a minute.
            guard.tick(now);
            // The recipes get the events of this batch (and of the API's edits since the last
            // turn), then the timers that are due fire. Their orders join the queue below.
            guard.run_recipes(monotonic_ms(), now);
        }
        // After the measures: the answer of an order (and the state it makes the device read
        // back) comes through the same polls.
        runner.step(&mut radio, &orders, monotonic_ms());
        if verbose {
            for event in &events {
                for line in event.lines() {
                    println!("{line}");
                }
            }
        }
        saver.tick(home, now, false)?;
    }
    Ok(true)
}
