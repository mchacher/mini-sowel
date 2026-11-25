//! `--serve`: start the network, keep the devices up to date from what they send, and serve
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
//! WebSocket client's thread takes the home once, to check its token, then the hub only, so no
//! client can hold up the radio loop or the API.

use std::io::{self, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use solite_core::api::{
    ListCursor, MAX_BODY_BYTES, MAX_WORKERS, Reach, Reply, Request, Slots, handle_guarded,
    is_streamed_request, order_response,
};
use solite_core::auth::AlwaysConfirmed;
use solite_core::backup::{ArchiveReader, MAX_ARCHIVE_BYTES};
use solite_core::devices::lock;
use solite_core::engine::{self, Clocks, DEFINITION_DELAY_MS, Saver};
use solite_core::home::Home;
use solite_core::radio::{Radio, RadioTransport, identity_lines, network_line};
use solite_core::update::{MAX_UPLOAD_BYTES, Updates};
use tiny_http::{Header, Response, Server};

use crate::backup_files::{DataFiles, HostPacing};
use crate::front::{self, Door};
use crate::random::SystemRandom;
use crate::static_files;
use crate::store::{FileStore, show_on_stderr};
use crate::update_files::{FileAdapter, start_updates};
use crate::ws::{Context, Timing};
use crate::{backup, clock};

/// The update key of this build (spec 032, R2): 64 hexadecimal characters, or absent.
const UPDATE_PUBLIC_KEY: Option<&str> = option_env!("SOLITE_UPDATE_PUBLIC_KEY");

/// What the API's workers need for the two streamed routes: the update's state and files, and
/// the data folder of the full backup.
struct UpdateParts {
    updates: Updates,
    adapter: FileAdapter,
    data: PathBuf,
}

/// What the probation reads (R5): the engine's loop is running, the API server is listening.
#[derive(Default)]
struct Health {
    engine: AtomicBool,
    listening: AtomicBool,
}

impl Health {
    fn healthy(&self) -> bool {
        self.engine.load(Ordering::SeqCst) && self.listening.load(Ordering::SeqCst)
    }
}

/// How often the probation is ticked.
const WATCH_TICK: Duration = Duration::from_secs(1);

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
/// `tiny_http` server on a loopback port of its own, whose address is returned. The port must
/// also be free on `[::1]`: a browser tries it first for `localhost`, so a program listening
/// there would capture the page meant for the host.
fn bind(port: u16) -> Result<(TcpListener, Arc<Server>, std::net::SocketAddr), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("serve: cannot listen on 127.0.0.1:{port}: {e}"))?;
    // Someone answering on `[::1]` is a clash, whatever address they bound (`[::1]` or the
    // wildcard `[::]` that Node uses): a bind test cannot see the wildcard, a connection can. A
    // computer without an IPv6 loopback, or nobody there, refuses the connection: no clash.
    let ipv6 = std::net::SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port));
    if std::net::TcpStream::connect_timeout(&ipv6, Duration::from_millis(300)).is_ok() {
        return Err(format!(
            "serve: cannot listen on [::1]:{port}: address already in use (another program answers there, `localhost` would reach it)"
        ));
    }
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
    update: &UpdateParts,
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
    let authorization = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.to_string());
    // The update's and the backup's bodies are files of megabytes: they are not read here, the
    // core answers the gate first and the file is then streamed (spec 032 R6, spec 035).
    let streaming = is_streamed_request(&method, &path);
    let backup_password = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Backup-Password"))
        .map(|h| h.value.to_string());
    // One byte more than allowed is enough to see that the body is too long.
    let mut body = Vec::new();
    if !streaming {
        let _ = request
            .as_reader()
            .take(MAX_BODY_BYTES as u64 + 1)
            .read_to_end(&mut body);
    }
    // The core takes and lets go of the home itself (a password hash runs without it).
    let reply = {
        let mut fill = |buffer: &mut [u8]| lock(random).fill(buffer);
        handle_guarded(
            home,
            &Request {
                method: &method,
                path: &path,
                host: host.as_deref(),
                port,
                reach: Reach::Loopback,
                content_type: request_type.as_deref(),
                authorization: authorization.as_deref(),
                body: &body,
            },
            now_ms(),
            &mut fill,
        )
    };
    // A download holds no body to skip, but is streamed all the same.
    if streaming || matches!(reply, Reply::BackupExport(_)) {
        return answer_streamed(
            request,
            reply,
            update,
            content_type,
            random,
            backup_password.as_deref(),
        );
    }
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
        Reply::Order(_)
        | Reply::Static(_)
        | Reply::StaticError(_)
        | Reply::Update
        | Reply::BackupExport(_)
        | Reply::BackupRestore { .. } => request.respond(Response::empty(500)),
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

/// The archive of an export as a chunked answer: no length, so the server sends it as it reads.
fn send_archive(request: tiny_http::Request, reader: ArchiveReader) {
    let header = |name: &str, value: &str| Header::from_bytes(name, value).ok();
    let headers: Vec<Header> = [
        header("Content-Type", "application/octet-stream"),
        header(
            "Content-Disposition",
            "attachment; filename=\"solite-backup.slb\"",
        ),
        header("Cache-Control", "no-store"),
    ]
    .into_iter()
    .flatten()
    .collect();
    // A read error ends the answer without its end: the client holds a truncated file, which its
    // MAC refuses. The client may have gone as well: nothing to do about it.
    let _ = request.respond(Response::new(
        tiny_http::StatusCode(200),
        headers,
        reader,
        None,
        None,
    ));
}

/// `POST /system/update` and the two backup routes, once the core has answered: a refusal of the
/// gate or of the content type, or the order to stream. The update's body is streamed from the
/// server's reader, bounded to the declared length, into `Updates::install`; the backup's, bounded
/// the same way, into `BackupTicket::restore`; an export streams the archive back, chunked. Every
/// refusal leaves body bytes unread, so the connection is closed after it (the door asks for it):
/// the file's bytes are never read as a next request.
fn answer_streamed(
    mut request: tiny_http::Request,
    reply: Reply,
    update: &UpdateParts,
    content_type: &Header,
    random: &Mutex<SystemRandom>,
    backup_password: Option<&str>,
) {
    let response = match reply {
        Reply::BackupExport(ticket) => {
            let mut fill = |buffer: &mut [u8]| lock(random).fill(buffer);
            match ticket.export(
                backup_password,
                Box::new(DataFiles::new(&update.data)),
                Box::new(HostPacing),
                &mut fill,
            ) {
                Ok(reader) => return send_archive(request, reader),
                Err(response) => response,
            }
        }
        Reply::BackupRestore { ticket, wifi } => {
            let declared = request.body_length().map(|n| n as u64);
            // Without a length (chunked) the stream is bounded all the same, one byte over
            // the limit being enough to see it is too long.
            let limit = declared.unwrap_or(MAX_ARCHIVE_BYTES as u64 + 1);
            let mut source = request.as_reader().take(limit);
            let mut files = DataFiles::new(&update.data);
            let response = ticket.restore(
                wifi,
                declared,
                backup_password,
                &mut source,
                &mut files,
                &mut HostPacing,
            );
            if response.status == 200 {
                println!(
                    "backup: restored {} files; restart the host to load them",
                    restored_files(response.body.as_deref()),
                );
            }
            response
        }
        Reply::Update => {
            let declared = request.body_length().map(|n| n as u64);
            if declared.is_some_and(|n| n > MAX_UPLOAD_BYTES) {
                solite_core::api::Response {
                    status: 413,
                    body: Some(r#"{"error":"Update too large"}"#.to_string()),
                }
            } else {
                // Without a length (chunked), the stream is bounded all the same.
                let limit = declared.unwrap_or(MAX_UPLOAD_BYTES);
                let mut source = request.as_reader().take(limit);
                let mut adapter = update.adapter.clone();
                let response = update.updates.install(&mut source, &mut adapter);
                if response.status == 200 {
                    println!(
                        "update: {} written to slot {}; restart the host to put it on probation",
                        installed_version(response.body.as_deref()),
                        adapter.boot_slot(),
                    );
                }
                response
            }
        }
        Reply::Whole(response) | Reply::StaticError(response) => response,
        Reply::List(_) | Reply::Order(_) | Reply::Static(_) => solite_core::api::Response {
            status: 500,
            body: Some(r#"{"error":"Internal error"}"#.to_string()),
        },
    };
    let answer = match response.body {
        Some(text) => Response::from_string(text)
            .with_status_code(response.status)
            .with_header(content_type.clone()),
        None => Response::from_string("")
            .with_status_code(response.status)
            .with_header(content_type.clone()),
    };
    // The client may have gone: nothing to do about it.
    let _ = request.respond(answer);
}

/// The `files` count of a restore's answer.
fn restored_files(body: Option<&str>) -> u64 {
    body.and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .and_then(|value| value["files"].as_u64())
        .unwrap_or_default()
}

/// `x.y.z` of the answer's `Installed x.y.z, restarting`.
fn installed_version(body: Option<&str>) -> String {
    body.and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .and_then(|value| value["message"].as_str().map(str::to_string))
        .and_then(|message| {
            message
                .strip_prefix("Installed ")
                .and_then(|rest| rest.split(',').next().map(str::to_string))
        })
        .unwrap_or_default()
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
    update: &Arc<UpdateParts>,
) {
    let slots = Slots::new(MAX_WORKERS);
    for request in server.incoming_requests() {
        let Some(guard) = slots.try_take() else {
            let _ = request.respond(
                Response::from_string(r#"{"error":"Busy"}"#)
                    .with_status_code(503)
                    .with_header(content_type.clone()),
            );
            continue;
        };
        let (home, random, content_type, ui, update) = (
            Arc::clone(home),
            Arc::clone(random),
            content_type.clone(),
            Arc::clone(ui),
            Arc::clone(update),
        );
        // The slot is released when the worker ends, or at once when the thread cannot start
        // (the closure, and the guard with it, is dropped).
        let _ = thread::Builder::new().spawn(move || {
            answer(request, &home, &random, port, &content_type, &ui, &update);
            drop(guard);
        });
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
        // At the host the person is at the keyboard: no button to press (spec 029 R6).
        guard.set_presence(AlwaysConfirmed);
        guard
            .pairing_mut()
            .set_grace_ms(u64::try_from(crate::close_grace().as_millis()).unwrap_or(u64::MAX));
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
            home: Arc::clone(&home),
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
    // The updates of this build and the files of the slots; the start lines, then the probation
    // ticks about once a second (R5), on a thread of its own that ends with the serve.
    let adapter = FileAdapter::new(data);
    let (updates, start_lines) = start_updates(UPDATE_PUBLIC_KEY, &adapter);
    for line in &start_lines {
        println!("{line}");
    }
    let update = Arc::new(UpdateParts {
        updates,
        adapter,
        data: data.to_path_buf(),
    });
    let health = Arc::new(Health::default());
    let watch_stop = Arc::new(AtomicBool::new(false));
    {
        let (update, health, watch_stop) = (
            Arc::clone(&update),
            Arc::clone(&health),
            Arc::clone(&watch_stop),
        );
        thread::spawn(move || {
            let mut adapter = update.adapter.clone();
            while !watch_stop.load(Ordering::SeqCst) {
                update
                    .updates
                    .watch(&mut adapter, monotonic_ms(), health.healthy());
                thread::sleep(WATCH_TICK);
            }
        });
    }
    {
        let (server, home, random, update) = (
            Arc::clone(&server),
            Arc::clone(&home),
            Arc::clone(&random),
            Arc::clone(&update),
        );
        let ui = Arc::new(ui.to_path_buf());
        thread::spawn(move || {
            http_loop(&server, &home, &random, port, &content_type, &ui, &update)
        });
    }
    // The server accepts connections from here on.
    health.listening.store(true, Ordering::SeqCst);
    // Ctrl-C before the server existed: nobody unblocked it.
    if stop.load(Ordering::SeqCst) {
        unblock();
    }
    if !static_files::is_built(ui) {
        println!("{}", static_files::missing_line(ui));
    }
    // A burst of the API's changes gives one write per file (spec 025 R3).
    let mut saver = Saver::new(store, now_ms())
        .with_sink(show_on_stderr)
        .with_definition_delay(DEFINITION_DELAY_MS);
    let result = radio_loop(
        transport, label, port, verbose, data, stop, &home, &random, &mut saver, &health,
    );
    watch_stop.store(true, Ordering::SeqCst);
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
    saver: &mut Saver<FileStore>,
    health: &Health,
) -> Result<bool, String> {
    let mut fill = |buffer: &mut [u8]| lock(random).fill(buffer);
    let mut radio = Radio::new(transport, clock());
    let identity = radio.identify().map_err(|e| format!("radio: {e}"))?;
    let lines = identity_lines(label, &identity);
    // The `network:` line of the identification is left out, as with `--listen`.
    for line in &lines[..3] {
        println!("{line}");
    }
    // The network is resumed, or formed on a radio that has none; joining stays closed until the
    // API opens it, and the devices are only followed while it is open (spec 017).
    let (network, foreign) = backup::start(&mut radio, data, &mut fill)?;
    radio.set_following(false);
    println!("{}", network_line(&network, foreign));
    println!("serving: http://127.0.0.1:{port}");
    let clocks = Clocks {
        now: Arc::new(now_ms),
        monotonic: Arc::new(monotonic_ms),
    };
    health.engine.store(true, Ordering::SeqCst);
    let result = engine::run(
        &mut radio,
        home,
        saver,
        &clocks,
        stop,
        &mut fill,
        verbose,
        &mut |line| println!("{line}"),
    )
    .map_err(String::from);
    health.engine.store(false, Ordering::SeqCst);
    result
}
